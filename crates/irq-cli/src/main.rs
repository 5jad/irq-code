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
        "check" | "emit-rust" => args.len() == 3,
        "build" => args.len() == 4,
        _ => false,
    };
    if !valid_args {
        emit_error(
            &[Diagnostic::error(
                codes::INVALID_USAGE,
                "usage_error",
                "usage: irq <check|emit-rust> <file.irq> | irq build <file.irq> <output.rlib>",
            )],
            &SourceMap::default(),
        );
        return ExitCode::from(2);
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
                "{{\"status\":\"ok\",\"phase\":\"mvp_subset\",\"checks\":[\"syntax_subset\",\"scalar_types\",\"immutability\",\"return_types\",\"if_else_flow\",\"while_subset\",\"constant_zero_division\",\"constant_integer_range\"],\"not_checked\":[\"contracts\",\"effects\",\"for_loops\",\"break\",\"loop_invariants\",\"general_dataflow\"],\"backend\":\"rust_subset_available_via_emit-rust_and_build\",\"generated_runtime_guards\":[\"integer_overflow\",\"division_by_zero\"],\"module\":{:?},\"functions\":{}}}",
                module.name,
                module.functions.len()
            );
            ExitCode::SUCCESS
        }
    }
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
