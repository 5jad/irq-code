use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let command = args.get(1).map(String::as_str).unwrap_or("");
    let valid_args = match command {
        "check" | "emit-rust" => args.len() == 3,
        "build" => args.len() == 4,
        _ => false,
    };
    if !valid_args {
        eprintln!("usage: irq <check|emit-rust> <file.irq> | irq build <file.irq> <output.rlib>");
        return ExitCode::from(2);
    }

    let source = match fs::read_to_string(&args[2]) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "{{\"status\":\"error\",\"kind\":\"io\",\"message\":{:?}}}",
                e.to_string()
            );
            return ExitCode::FAILURE;
        }
    };
    let module = match irq_parser::parse(&source) {
        Ok(module) => module,
        Err(e) => {
            println!(
                "{{\"status\":\"error\",\"code\":{:?},\"kind\":\"parse_error\",\"line\":{},\"message\":{:?}}}",
                e.code, e.line, e.message
            );
            return ExitCode::FAILURE;
        }
    };
    if let Err(errors) = irq_check::check(&module) {
        print!("{{\"status\":\"error\",\"kind\":\"type_errors\",\"diagnostics\":[");
        for (i, e) in errors.iter().enumerate() {
            if i > 0 {
                print!(",");
            }
            print!(
                "{{\"code\":{:?},\"function\":{:?},\"message\":{:?}}}",
                e.code, e.function, e.message
            );
        }
        println!("]}}");
        return ExitCode::FAILURE;
    }

    match command {
        "emit-rust" => match irq_codegen_rust::emit(&module) {
            Ok(code) => {
                print!("{code}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!(
                    "{{\"status\":\"error\",\"kind\":\"codegen_error\",\"message\":{:?}}}",
                    error.to_string()
                );
                ExitCode::FAILURE
            }
        },
        "build" => build_rust(&module, &args[3]),
        _ => {
            println!(
                "{{\"status\":\"ok\",\"phase\":\"mvp_subset\",\"checks\":[\"syntax_subset\",\"scalar_types\",\"immutability\",\"return_types\",\"if_else_flow\",\"constant_zero_division\",\"constant_integer_range\"],\"not_checked\":[\"contracts\",\"effects\",\"loops\",\"general_dataflow\"],\"backend\":\"rust_subset_available_via_emit-rust_and_build\",\"generated_runtime_guards\":[\"integer_overflow\",\"division_by_zero\"],\"module\":{:?},\"functions\":{}}}",
                module.name,
                module.functions.len()
            );
            ExitCode::SUCCESS
        }
    }
}

fn build_rust(module: &irq_ast::Module, output: &str) -> ExitCode {
    let generated = match irq_codegen_rust::emit(module) {
        Ok(code) => code,
        Err(error) => {
            eprintln!(
                "{{\"status\":\"error\",\"kind\":\"codegen_error\",\"message\":{:?}}}",
                error.to_string()
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
        eprintln!(
            "{{\"status\":\"error\",\"kind\":\"io\",\"message\":{:?}}}",
            error.to_string()
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
                eprintln!(
                    "{{\"status\":\"error\",\"kind\":\"artifact_commit_error\",\"message\":{:?}}}",
                    error.to_string()
                );
                ExitCode::FAILURE
            }
        },
        Ok(result) => {
            let _ = fs::remove_file(&temp_output);
            eprintln!(
                "{{\"status\":\"error\",\"kind\":\"rustc_error\",\"message\":{:?}}}",
                String::from_utf8_lossy(&result.stderr)
            );
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!(
                "{{\"status\":\"error\",\"kind\":\"rustc_unavailable\",\"message\":{:?}}}",
                error.to_string()
            );
            ExitCode::FAILURE
        }
    }
}
