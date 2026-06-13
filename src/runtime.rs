use rquickjs::function::{Async, Func, Rest};
use rquickjs::{
    AsyncContext, AsyncRuntime, CatchResultExt, CaughtError, Ctx, Exception, FromJs, Function,
    Module, Object, Value,
};

const BOOTSTRAP_SOURCE: &str =
    "delete Object.prototype.__proto__;\nObject.setPrototypeOf($, null);\nObject.freeze($);";

pub fn run_script(source_code: &str, filename: &str) -> Result<(), String> {
    eval_script(source_code, filename).map(|_| ())
}

pub fn eval_script(source_code: &str, filename: &str) -> Result<String, String> {
    pollster::block_on(eval_script_async(source_code, filename))
}

async fn eval_script_async(source_code: &str, filename: &str) -> Result<String, String> {
    let runtime = AsyncRuntime::new().map_err(|err| err.to_string())?;
    let context = AsyncContext::full(&runtime)
        .await
        .map_err(|err| err.to_string())?;

    context
        .async_with(async |ctx| {
            RuntimeContext::new(ctx)
                .initialize()?
                .evaluate_async_script(source_code, filename)
                .await
        })
        .await
}

struct RuntimeContext<'js> {
    ctx: Ctx<'js>,
}

impl<'js> RuntimeContext<'js> {
    fn new(ctx: Ctx<'js>) -> Self {
        Self { ctx }
    }

    fn initialize(self) -> Result<Self, String> {
        self.install_console()?;
        self.install_shell_tag()?;
        self.run_bootstrap()?;
        Ok(self)
    }

    async fn evaluate_async_script(
        self,
        source_code: &str,
        filename: &str,
    ) -> Result<String, String> {
        let promise = Module::evaluate(self.ctx.clone(), filename, source_code)
            .catch(&self.ctx)
            .map_err(|err| self.format_caught_error(err))?;
        promise
            .into_future::<()>()
            .await
            .catch(&self.ctx)
            .map_err(|err| self.format_caught_error(err))?;

        Ok("undefined".to_string())
    }

    fn install_console(&self) -> Result<(), String> {
        let console = Object::new(self.ctx.clone()).map_err(|err| err.to_string())?;
        console
            .set("log", Func::from(console_log))
            .map_err(|err| err.to_string())?;
        self.ctx
            .globals()
            .set("console", console)
            .map_err(|err| err.to_string())
    }

    fn install_shell_tag(&self) -> Result<(), String> {
        self.ctx
            .globals()
            .set("$", Func::from(Async(shell_tag_callback)))
            .map_err(|err| err.to_string())
    }

    fn run_bootstrap(&self) -> Result<(), String> {
        self.eval::<()>(BOOTSTRAP_SOURCE, "<bootstrap>")
    }

    fn eval<T>(&self, source_code: impl Into<Vec<u8>>, filename: &str) -> Result<T, String>
    where
        T: FromJs<'js>,
    {
        self.ctx
            .eval_with_options::<T, _>(source_code, eval_options(filename))
            .catch(&self.ctx)
            .map_err(|err| self.format_caught_error(err))
    }

    fn format_caught_error(&self, err: CaughtError<'js>) -> String {
        match err {
            CaughtError::Exception(exception) => exception
                .message()
                .or_else(|| exception.stack())
                .unwrap_or_else(|| "JavaScript exception".to_string()),
            CaughtError::Value(value) => format_js_value_quick(&self.ctx, value)
                .unwrap_or_else(|_| "JavaScript threw a non-stringable value".to_string()),
            CaughtError::Error(err) => err.to_string(),
        }
    }
}

fn console_log<'js>(ctx: Ctx<'js>, args: Rest<Value<'js>>) -> rquickjs::Result<()> {
    let values = args
        .0
        .into_iter()
        .map(|value| format_js_value_quick(&ctx, value))
        .collect::<rquickjs::Result<Vec<_>>>()?;
    println!("{}", values.join(" "));
    Ok(())
}

async fn shell_tag_callback<'js>(
    ctx: Ctx<'js>,
    args: Rest<Value<'js>>,
) -> rquickjs::Result<String> {
    let command = build_shell_command(&ctx, args.0)?;
    crate::shell::run(&command).map_err(|err| Exception::throw_message(&ctx, &err))
}

fn build_shell_command<'js>(ctx: &Ctx<'js>, args: Vec<Value<'js>>) -> rquickjs::Result<String> {
    let strings = args
        .first()
        .and_then(Value::as_object)
        .ok_or_else(|| Exception::throw_type(ctx, "$ must be used as a tagged template"))?;
    let mut command = String::new();

    for index in 0..args.len() {
        let segment = strings.get::<_, Value<'_>>(index as u32)?;
        command.push_str(&format_js_value_quick(ctx, segment)?);

        let arg_index = index + 1;
        if arg_index < args.len() {
            command.push_str(&format_js_value_quick(ctx, args[arg_index].clone())?);
        }
    }

    Ok(command)
}

fn format_js_value_quick<'js>(ctx: &Ctx<'js>, value: Value<'js>) -> rquickjs::Result<String> {
    let string_ctor: Function<'js> = ctx.globals().get("String")?;
    string_ctor.call((value,))
}

fn eval_options(filename: &str) -> rquickjs::context::EvalOptions {
    let mut options = rquickjs::context::EvalOptions::default();
    options.filename = Some(filename.to_string());
    options
}

#[cfg(test)]
mod tests {
    use super::eval_script;

    #[test]
    fn evaluates_script_source_and_returns_result() {
        let result = eval_script(
            "const expected = '[\"Hello from hello.js\",\"hello\\\\n\",\"undefined\",\"null\",\"undefined:undefined\"]';\n\
             const results = [];\n\
             results.push('Hello from hello.js');\n\
             results.push(await $`echo hello`);\n\
             try { $.tsh = 'polluted'; } catch {}\n\
             results.push(String($.tsh));\n\
             results.push(String(Object.getPrototypeOf($)));\n\
             const value = {};\n\
             value.__proto__ = { polluted: true };\n\
             results.push(`${Object.prototype.__proto__}:${value.polluted}`);\n\
             const actual = JSON.stringify(results);\n\
             if (actual !== expected) {\n\
                 throw new Error(actual);\n\
             }",
            "<test>",
        )
        .unwrap();

        assert_eq!(result, "undefined");
    }

    #[test]
    fn rejects_failed_shell_command() {
        let err = eval_script("await $`tsh-command-that-does-not-exist`;", "<test>").unwrap_err();

        assert!(err.contains("failed to execute shell command"));
    }

    #[test]
    fn propagates_async_rejections() {
        let err = eval_script(
            "await Promise.reject(new Error('async rejection works'));",
            "<test>",
        )
        .unwrap_err();

        assert!(err.contains("async rejection works"));
    }

    #[test]
    fn rejects_top_level_return() {
        let err = eval_script("return 1;", "<test>").unwrap_err();

        assert!(err.contains("return"));
    }
}
