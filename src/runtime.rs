pub fn run_script(source_code: &str, filename: &str) -> Result<(), String> {
    eval_script(source_code, filename).map(|_| ())
}

pub fn eval_script(source_code: &str, filename: &str) -> Result<String, String> {
    let platform = v8::new_default_platform(0, false).make_shared();
    v8::V8::initialize_platform(platform);
    v8::V8::initialize();

    let result = {
        let isolate = &mut v8::Isolate::new(v8::CreateParams::default());
        isolate.set_microtasks_policy(v8::MicrotasksPolicy::Explicit);

        v8::scope!(let handle_scope, isolate);
        let context = v8::Context::new(handle_scope, Default::default());
        let scope = &mut v8::ContextScope::new(handle_scope, context);

        install_globals(scope, context)?;
        run_bootstrap(scope)?;
        execute_async_script(scope, source_code, filename)
    };

    unsafe {
        v8::V8::dispose();
    }
    v8::V8::dispose_platform();

    result
}

fn install_globals(
    scope: &mut v8::PinScope,
    context: v8::Local<v8::Context>,
) -> Result<(), String> {
    install_console(scope, context)?;
    install_shell_tag(scope, context)
}

fn run_bootstrap(scope: &mut v8::PinScope) -> Result<(), String> {
    execute_script(scope, "delete Object.prototype.__proto__;", "<bootstrap>")
}

fn execute_script(
    scope: &mut v8::PinScope,
    source_code: &str,
    filename: &str,
) -> Result<(), String> {
    let source =
        v8::String::new(scope, source_code).ok_or_else(|| "failed to create source".to_string())?;
    let filename =
        v8::String::new(scope, filename).ok_or_else(|| "failed to create filename".to_string())?;
    let origin = v8::ScriptOrigin::new(
        scope,
        filename.into(),
        0,
        0,
        false,
        0,
        None,
        false,
        false,
        false,
        None,
    );

    let script = v8::Script::compile(scope, source, Some(&origin))
        .ok_or_else(|| "failed to compile script".to_string())?;
    script
        .run(scope)
        .ok_or_else(|| "failed to run script".to_string())?;

    Ok(())
}

fn execute_async_script(
    scope: &mut v8::PinScope,
    source_code: &str,
    filename: &str,
) -> Result<String, String> {
    let wrapped_source = format!("(async () => {{\n{source_code}\n}})()");
    let source = v8::String::new(scope, &wrapped_source)
        .ok_or_else(|| "failed to create source".to_string())?;
    let filename =
        v8::String::new(scope, filename).ok_or_else(|| "failed to create filename".to_string())?;
    let origin = v8::ScriptOrigin::new(
        scope,
        filename.into(),
        0,
        0,
        false,
        0,
        None,
        false,
        false,
        false,
        None,
    );

    let script = v8::Script::compile(scope, source, Some(&origin))
        .ok_or_else(|| "failed to compile script".to_string())?;
    let value = script
        .run(scope)
        .ok_or_else(|| "failed to run script".to_string())?;

    scope.perform_microtask_checkpoint();

    if value.is_promise() {
        let promise = unsafe { v8::Local::<v8::Promise>::cast_unchecked(value) };
        match promise.state() {
            v8::PromiseState::Fulfilled => format_js_value(scope, promise.result(scope))
                .ok_or_else(|| "script promise fulfilled with non-stringable value".to_string()),
            v8::PromiseState::Rejected => Err(format_js_value(scope, promise.result(scope))
                .unwrap_or_else(|| {
                    "script promise rejected with non-stringable value".to_string()
                })),
            v8::PromiseState::Pending => Err("script promise is still pending".to_string()),
        }
    } else {
        format_js_value(scope, value)
            .ok_or_else(|| "script returned non-stringable value".to_string())
    }
}

fn install_console(
    scope: &mut v8::PinScope,
    context: v8::Local<v8::Context>,
) -> Result<(), String> {
    let console = v8::Object::new(scope);
    let log_name =
        v8::String::new(scope, "log").ok_or_else(|| "failed to create log".to_string())?;
    let log = v8::Function::new(scope, console_log)
        .ok_or_else(|| "failed to create console.log".to_string())?;
    console
        .set(scope, log_name.into(), log.into())
        .filter(|set| *set)
        .map(|_| ())
        .ok_or_else(|| "failed to set console.log".to_string())?;

    let console_name =
        v8::String::new(scope, "console").ok_or_else(|| "failed to create console".to_string())?;
    context
        .global(scope)
        .set(scope, console_name.into(), console.into())
        .filter(|set| *set)
        .map(|_| ())
        .ok_or_else(|| "failed to set console".to_string())
}

fn install_shell_tag(
    scope: &mut v8::PinScope,
    context: v8::Local<v8::Context>,
) -> Result<(), String> {
    let shell_tag_name =
        v8::String::new(scope, "$").ok_or_else(|| "failed to create $".to_string())?;
    let shell_tag = v8::Function::new(scope, shell_tag_callback)
        .ok_or_else(|| "failed to create $ function".to_string())?;
    let null = v8::null(scope);
    shell_tag
        .set_prototype(scope, null.into())
        .filter(|set| *set)
        .map(|_| ())
        .ok_or_else(|| "failed to set $ prototype".to_string())?;
    shell_tag
        .set_integrity_level(scope, v8::IntegrityLevel::Frozen)
        .filter(|set| *set)
        .map(|_| ())
        .ok_or_else(|| "failed to freeze $".to_string())?;

    context
        .global(scope)
        .set(scope, shell_tag_name.into(), shell_tag.into())
        .filter(|set| *set)
        .map(|_| ())
        .ok_or_else(|| "failed to set $".to_string())
}

fn console_log(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
    let values = (0..args.length())
        .map(|i| format_js_value(scope, args.get(i)).unwrap_or_default())
        .collect::<Vec<_>>();
    println!("{}", values.join(" "));
}

fn shell_tag_callback(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    retval: v8::ReturnValue,
) {
    let command = build_shell_command(scope, args);
    resolve_shell_command(scope, retval, command);
}

fn resolve_shell_command(
    scope: &mut v8::PinScope,
    mut retval: v8::ReturnValue,
    command: Result<String, String>,
) {
    let resolver = match v8::PromiseResolver::new(scope) {
        Some(resolver) => resolver,
        None => return,
    };
    let promise = resolver.get_promise(scope);
    retval.set(promise.into());

    match command.and_then(|command| crate::shell::run(&command)) {
        Ok(stdout) => {
            let value = v8::String::new(scope, &stdout)
                .map(Into::into)
                .unwrap_or_else(|| v8::undefined(scope).into());
            let _ = resolver.resolve(scope, value);
        }
        Err(err) => {
            let message = v8::String::new(scope, &err).unwrap_or_else(|| v8::String::empty(scope));
            let error = v8::Exception::error(scope, message);
            let _ = resolver.reject(scope, error);
        }
    }
}

fn build_shell_command(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
) -> Result<String, String> {
    let strings = args
        .get(0)
        .to_object(scope)
        .ok_or_else(|| "$ must be used as a tagged template".to_string())?;
    let mut command = String::new();

    for index in 0..args.length() as u32 {
        let segment = strings
            .get_index(scope, index)
            .and_then(|value| format_js_value(scope, value))
            .ok_or_else(|| "failed to read shell template segment".to_string())?;
        command.push_str(&segment);

        let arg_index = index + 1;
        if arg_index < args.length() as u32 {
            let interpolation = format_js_value(scope, args.get(arg_index as i32))
                .ok_or_else(|| "failed to read shell template interpolation".to_string())?;
            command.push_str(&interpolation);
        }
    }

    Ok(command)
}

fn format_js_value(scope: &mut v8::PinScope, value: v8::Local<v8::Value>) -> Option<String> {
    value
        .to_string(scope)
        .map(|value| value.to_rust_string_lossy(scope))
}

#[cfg(test)]
mod tests {
    use super::eval_script;

    #[test]
    fn evaluates_script_source_and_returns_result() {
        let result = eval_script(
            "const results = [];\n\
             results.push('Hello from hello.js');\n\
             results.push(await $`echo hello`);\n\
             try { $.tsh = 'polluted'; } catch {}\n\
             results.push(String($.tsh));\n\
             results.push(String(Object.getPrototypeOf($)));\n\
             const value = {};\n\
             value.__proto__ = { polluted: true };\n\
             results.push(`${Object.prototype.__proto__}:${value.polluted}`);\n\
             return JSON.stringify(results);",
            "<test>",
        )
        .unwrap()
        .replace("\\r\\n", "\\n");

        assert_eq!(
            result,
            r#"["Hello from hello.js","hello\n","undefined","null","undefined:undefined"]"#
        );
    }
}
