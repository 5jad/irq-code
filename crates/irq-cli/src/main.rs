use std::{
    env, fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

use irq_diagnostics::{ByteSpan, Diagnostic, Label, SourceMap, codes, render_json};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let command = args.get(1).map(String::as_str).unwrap_or("");
    let valid_args = match command {
        "check" | "check-project" | "emit-rust" | "fmt" => args.len() == 3,
        "build" => args.len() == 4,
        _ => false,
    };
    if !valid_args {
        emit_error(
            &[Diagnostic::error(
                codes::INVALID_USAGE,
                "usage_error",
                "usage: irq <check|check-project|emit-rust|fmt> <file.irq|project-dir> | irq build <file.irq> <output.rlib>",
            )],
            &SourceMap::default(),
        );
        return ExitCode::from(2);
    }

    if command == "check-project" {
        return check_project(&args[2]);
    }

    let input_path = PathBuf::from(&args[2]);
    if let Ok(metadata) = fs::metadata(&input_path)
        && metadata.len() > irq_lexer::MAX_SOURCE_BYTES as u64
    {
        emit_error(
            &[Diagnostic::error(
                codes::SOURCE_TOO_LARGE,
                "resource_limit",
                format!(
                    "source exceeds the {} byte limit",
                    irq_lexer::MAX_SOURCE_BYTES
                ),
            )],
            &SourceMap::default(),
        );
        return ExitCode::FAILURE;
    }
    let source = match read_source(&input_path) {
        Ok(source) => source,
        Err(error) => {
            let oversized = error.to_string().starts_with("source exceeds the ");
            emit_error(
                &[Diagnostic::error(
                    if oversized {
                        codes::SOURCE_TOO_LARGE
                    } else {
                        codes::IO_ERROR
                    },
                    if oversized {
                        "resource_limit"
                    } else {
                        "io_error"
                    },
                    format!("cannot read `{}`: {error}", input_path.display()),
                )],
                &SourceMap::default(),
            );
            return ExitCode::FAILURE;
        }
    };
    let mut sources = SourceMap::default();
    let file_id = sources.add_file(input_path.to_string_lossy().into_owned(), source.clone());

    let module = match irq_parser::parse(&source) {
        Ok(module) => module,
        Err(error) => {
            let diagnostic = Diagnostic::error(error.code, "parse_error", error.message);
            let diagnostic = match error.span {
                Some(span) => diagnostic.with_label(Label::primary(
                    file_id,
                    ByteSpan {
                        start: span.start,
                        end: span.end,
                    },
                    "here",
                )),
                None => add_line_label(diagnostic, &sources, file_id, error.line),
            };
            emit_error(&[diagnostic], &sources);
            return ExitCode::FAILURE;
        }
    };
    if command == "fmt" {
        print!("{}", format_source(&source));
        return ExitCode::SUCCESS;
    }
    if let Err(errors) = irq_check::check(&module) {
        let diagnostics: Vec<_> = errors
            .into_iter()
            .map(|error| {
                let diagnostic = Diagnostic::error(error.code, "type_error", error.message);
                match module.functions.iter().find(|f| f.name == error.function) {
                    Some(function) => add_line_label(diagnostic, &sources, file_id, function.line),
                    None => diagnostic,
                }
            })
            .collect();
        emit_error(&diagnostics, &sources);
        return ExitCode::FAILURE;
    }
    if let Err(error) = irq_hir::lower(&module) {
        emit_error(
            &[Diagnostic::error(
                codes::TYPE_EXPRESSION,
                "hir_lowering_error",
                error.to_string(),
            )],
            &sources,
        );
        return ExitCode::FAILURE;
    }

    match command {
        "emit-rust" => match irq_codegen_rust::emit(&module) {
            Ok(code) => {
                print!("{code}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                emit_error(
                    &[Diagnostic::error(
                        codes::CODEGEN_ERROR,
                        "codegen_error",
                        error.to_string(),
                    )],
                    &sources,
                );
                ExitCode::FAILURE
            }
        },
        "build" => build_rust(&module, &args[3], &sources),
        _ => {
            println!(
                "{{\"status\":\"ok\",\"phase\":\"mvp_subset\",\"checks\":[\"syntax_subset\",\"scalar_types\",\"immutability\",\"return_types\",\"if_else_flow\",\"while_subset\",\"constant_zero_division\",\"constant_integer_range\",\"typed_hir_lowering\"],\"not_checked\":[\"contracts\",\"effects\",\"for_loops\",\"break\",\"loop_invariants\",\"general_dataflow\",\"ownership_and_borrowing\",\"multi_module_resolution\"],\"backend\":\"rust_subset_available_via_emit-rust_and_build\",\"generated_runtime_guards\":[\"integer_overflow\",\"division_by_zero\"],\"module\":{:?},\"functions\":{}}}",
                module.name,
                module.functions.len()
            );
            ExitCode::SUCCESS
        }
    }
}

fn check_project(root: &str) -> ExitCode {
    let project = match irq_project::load_project(root) {
        Ok(project) => project,
        Err(error) => {
            let mut sources = SourceMap::default();
            let kind = if error.relative_path.is_some() {
                "parse_error"
            } else {
                "project_error"
            };
            let diagnostic = Diagnostic::error(error.code, kind, error.message);
            let diagnostic = if let Some(relative) = error.relative_path {
                let full_path = Path::new(root).join(&relative);
                match read_source(&full_path) {
                    Ok(source) => {
                        let file_id =
                            sources.add_file(relative.to_string_lossy().into_owned(), source);
                        if let Some(span) = error.span {
                            diagnostic.with_label(Label::primary(
                                file_id,
                                ByteSpan {
                                    start: span.start,
                                    end: span.end,
                                },
                                "here",
                            ))
                        } else {
                            add_line_label(diagnostic, &sources, file_id, error.line.unwrap_or(1))
                        }
                    }
                    Err(_) => diagnostic,
                }
            } else {
                diagnostic
            };
            emit_error(&[diagnostic], &sources);
            return ExitCode::FAILURE;
        }
    };

    let mut sources = SourceMap::default();
    let mut diagnostics = Vec::new();
    for loaded in &project.modules {
        let path = project.root.join(&loaded.relative_path);
        let source = match read_source(&path) {
            Ok(source) => source,
            Err(error) => {
                diagnostics.push(Diagnostic::error(
                    codes::IO_ERROR,
                    "io_error",
                    format!("cannot read `{}`: {error}", loaded.relative_path.display()),
                ));
                continue;
            }
        };
        let file_id = sources.add_file(loaded.relative_path.to_string_lossy().into_owned(), source);
        match irq_check::check(&loaded.module) {
            Ok(()) => {
                if let Err(error) = irq_hir::lower(&loaded.module) {
                    diagnostics.push(Diagnostic::error(
                        codes::TYPE_EXPRESSION,
                        "hir_lowering_error",
                        format!("module `{}`: {error}", loaded.module.name),
                    ));
                }
            }
            Err(errors) => {
                diagnostics.extend(errors.into_iter().map(|error| {
                    let diagnostic = Diagnostic::error(error.code, "type_error", error.message);
                    match loaded
                        .module
                        .functions
                        .iter()
                        .find(|f| f.name == error.function)
                    {
                        Some(function) => {
                            add_line_label(diagnostic, &sources, file_id, function.line)
                        }
                        None => diagnostic,
                    }
                }));
            }
        }
    }
    if !diagnostics.is_empty() {
        emit_error(&diagnostics, &sources);
        return ExitCode::FAILURE;
    }
    println!(
        "{{\"status\":\"ok\",\"kind\":\"project_check\",\"phase\":\"per_module_subset\",\"package\":{:?},\"modules\":{},\"not_checked\":[\"cross_module_resolution\",\"imports\",\"privacy\",\"contracts\",\"effects\"]}}",
        project.manifest.name,
        project.modules.len()
    );
    ExitCode::SUCCESS
}

/// Applies a stable whitespace-only format after parsing has established syntax validity.
fn format_source(source: &str) -> String {
    let normalized = source.replace("\r\n", "\n");
    let mut lines = Vec::new();
    let mut previous_blank = true;
    for raw in normalized.lines() {
        let line = trim_trailing_space_outside_literals(raw);
        let blank = line.trim().is_empty();
        if blank {
            if !previous_blank && !lines.is_empty() {
                lines.push(String::new());
            }
            previous_blank = true;
            continue;
        }
        let trimmed = line.trim_start();
        let top_level_function =
            line == trimmed && (trimmed.starts_with("fn ") || trimmed.starts_with("pub fn "));
        if top_level_function && !lines.is_empty() && !previous_blank {
            lines.push(String::new());
        }
        lines.push(line);
        previous_blank = false;
    }
    while lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    }
}

fn trim_trailing_space_outside_literals(line: &str) -> String {
    let mut quoted = false;
    let mut escaped = false;
    let mut last_significant_end = 0;
    for (offset, ch) in line.char_indices() {
        if quoted {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                quoted = false;
                last_significant_end = offset + ch.len_utf8();
            } else {
                last_significant_end = offset + ch.len_utf8();
            }
        } else if ch == '"' {
            quoted = true;
            last_significant_end = offset + ch.len_utf8();
        } else if ch == '#' {
            last_significant_end = line.trim_end().len();
            break;
        } else if !ch.is_whitespace() {
            last_significant_end = offset + ch.len_utf8();
        }
    }
    line[..last_significant_end].to_owned()
}

fn read_source(path: &Path) -> std::io::Result<String> {
    let file = fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take((irq_lexer::MAX_SOURCE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > irq_lexer::MAX_SOURCE_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "source exceeds the {} byte limit",
                irq_lexer::MAX_SOURCE_BYTES
            ),
        ));
    }
    String::from_utf8(bytes)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

fn add_line_label(
    diagnostic: Diagnostic,
    sources: &SourceMap,
    file_id: irq_diagnostics::FileId,
    line: usize,
) -> Diagnostic {
    match sources.line_span(file_id, line) {
        Some(span) => diagnostic.with_label(Label::primary(file_id, span, "here")),
        None => diagnostic,
    }
}

fn emit_error(diagnostics: &[Diagnostic], sources: &SourceMap) {
    println!("{}", render_json(diagnostics, sources));
}

fn build_rust(module: &irq_ast::Module, output: &str, sources: &SourceMap) -> ExitCode {
    let generated = match irq_codegen_rust::emit(module) {
        Ok(code) => code,
        Err(error) => {
            emit_error(
                &[Diagnostic::error(
                    codes::CODEGEN_ERROR,
                    "codegen_error",
                    error.to_string(),
                )],
                sources,
            );
            return ExitCode::FAILURE;
        }
    };
    let temp_source = env::temp_dir().join(format!("irq_{}_generated.rs", std::process::id()));
    let output_path = PathBuf::from(output);
    let output_dir = output_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = output_path
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new("irq-output"))
        .to_string_lossy();
    let temp_output = output_dir.join(format!(".{file_name}.irq-tmp-{}.rlib", std::process::id()));
    if let Err(error) = fs::write(&temp_source, generated) {
        emit_error(
            &[Diagnostic::error(
                codes::IO_ERROR,
                "io_error",
                format!("cannot write temporary source: {error}"),
            )],
            sources,
        );
        return ExitCode::FAILURE;
    }
    let compile = Command::new("rustc")
        .arg("--crate-type")
        .arg("lib")
        .arg("--crate-name")
        .arg("irq_generated")
        .arg("-D")
        .arg("warnings")
        .arg(&temp_source)
        .arg("-o")
        .arg(&temp_output)
        .output();
    let _ = fs::remove_file(&temp_source);
    match compile {
        Ok(result) if result.status.success() => match fs::rename(&temp_output, &output_path) {
            Ok(()) => {
                println!(
                    "{{\"status\":\"ok\",\"kind\":\"build\",\"target\":\"rust_library\",\"artifact\":{:?},\"module\":{:?}}}",
                    output, module.name
                );
                ExitCode::SUCCESS
            }
            Err(error) => {
                let _ = fs::remove_file(&temp_output);
                emit_error(
                    &[Diagnostic::error(
                        codes::ARTIFACT_COMMIT_ERROR,
                        "artifact_commit_error",
                        format!("cannot atomically publish artifact: {error}"),
                    )],
                    sources,
                );
                ExitCode::FAILURE
            }
        },
        Ok(result) => {
            let _ = fs::remove_file(&temp_output);
            emit_error(
                &[Diagnostic::error(
                    codes::RUSTC_ERROR,
                    "rustc_error",
                    String::from_utf8_lossy(&result.stderr).into_owned(),
                )],
                sources,
            );
            ExitCode::FAILURE
        }
        Err(error) => {
            emit_error(
                &[Diagnostic::error(
                    codes::RUSTC_UNAVAILABLE,
                    "rustc_unavailable",
                    error.to_string(),
                )],
                sources,
            );
            ExitCode::FAILURE
        }
    }
}
