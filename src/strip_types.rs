use oxc_allocator::Allocator;
use oxc_codegen::{Codegen, CodegenOptions};
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::SourceType;
use oxc_transformer::{TransformOptions, Transformer};
use std::path::Path;

pub fn transform(source_code: &str, filename: &str) -> Result<String, String> {
    let allocator = Allocator::default();
    let source_type = SourceType::from_path(Path::new(filename))
        .unwrap_or_default()
        .with_typescript(true);
    let parser_return = Parser::new(&allocator, source_code, source_type).parse();

    if let Some(error) = parser_return.errors.first() {
        return Err(format!(
            "failed to parse TypeScript in {filename}: {error:?}"
        ));
    }

    let mut program = parser_return.program;
    let scoping = SemanticBuilder::new()
        .build(&program)
        .semantic
        .into_scoping();
    let transform_options = TransformOptions::default();
    let transformer_return = Transformer::new(&allocator, Path::new(filename), &transform_options)
        .build_with_scoping(scoping, &mut program);

    if let Some(error) = transformer_return.errors.first() {
        return Err(format!(
            "failed to transform TypeScript in {filename}: {error:?}"
        ));
    }

    Ok(Codegen::new()
        .with_options(CodegenOptions::default())
        .build(&program)
        .code)
}

#[cfg(test)]
mod tests {
    use super::transform;

    #[test]
    fn strips_type_annotations() {
        let output = transform("const answer: number = 42;", "test.ts").unwrap();

        assert!(!output.contains(": number"));
        assert!(output.contains("const answer = 42"));
    }

    #[test]
    fn strips_type_only_declarations() {
        let output = transform(
            "interface Person { name: string }\ntype Age = number;\nconst name: string = 'Sato';",
            "test.ts",
        )
        .unwrap();

        assert!(!output.contains("interface"));
        assert!(!output.contains("type Age"));
        assert!(output.contains("const name"));
        assert!(output.contains("Sato"));
    }
}
