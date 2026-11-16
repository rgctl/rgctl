//! Adversarial field-name source spans discovered during correctness review.
use rgctl_extraction::{Extractor, GraphBuilder};
use rgctl_graph::schema::Node;
use std::{path::Path, sync::Arc};

fn fields(file: &str, source: &str) -> Vec<Node> {
    let extractor = Extractor::new(Arc::new(rgctl_languages::default_registry()));
    let extraction = extractor
        .extract_file_with_source(Path::new(file), source.as_bytes().to_vec())
        .unwrap();
    let mut builder = GraphBuilder::new();
    builder.set_materialize_fields(true);
    extractor
        .populate_graph(&[extraction], &mut builder)
        .unwrap();
    builder
        .into_graph()
        .0
        .into_iter()
        .filter(|node| node.has_label("field"))
        .collect()
}

fn identifier_range(node: &Node) -> (usize, usize) {
    let (relative, len) = node
        .name_span_overflow
        .map(|(start, len)| (start as usize, len as usize))
        .unwrap_or_else(|| {
            (
                node.name_rel_offset.unwrap() as usize,
                node.name_token_len
                    .map(usize::from)
                    .unwrap_or(node.name.len()),
            )
        });
    let start = node.start_byte.unwrap() as usize + relative;
    (start, start + len)
}

#[test]
fn java_annotations_and_record_names_cannot_replace_field_identifier_spans() {
    let source = "class Example { @Deprecated int Deprecated; int first = this.second, second; }\nrecord Point(int Point) {}\n";
    let fields = fields("Example.java", source);
    for (qualified, expected) in [
        (
            "Example.Deprecated",
            source.find("int Deprecated").unwrap() + 4,
        ),
        ("Example.second", source.find("second;").unwrap()),
        ("Point.Point", source.find("int Point").unwrap() + 4),
    ] {
        let field = fields
            .iter()
            .find(|node| node.qualified_name.as_deref() == Some(qualified))
            .unwrap();
        let (start, end) = identifier_range(field);
        assert_eq!(start, expected, "{qualified}");
        assert_eq!(&source[start..end], field.name.as_str());
        assert!(
            field.start_byte.unwrap() as usize <= start && end <= field.end_byte.unwrap() as usize
        );
    }
    let component = fields
        .iter()
        .find(|node| node.qualified_name.as_deref() == Some("Point.Point"))
        .unwrap();
    assert_eq!(
        &source[component.start_byte.unwrap() as usize..component.end_byte.unwrap() as usize],
        "int Point"
    );
}

#[test]
fn typescript_decorator_name_cannot_replace_the_property_identifier() {
    let source = "class Example { @value value: number = 1; }\n";
    let fields = fields("example.ts", source);
    let field = fields.iter().find(|node| node.name == "value").unwrap();
    let (start, end) = identifier_range(field);
    assert_eq!(start, source.find("value:").unwrap());
    assert_eq!(&source[start..end], "value");
}

#[test]
fn elixir_quoted_struct_members_retain_identifier_spans() {
    let source = "defmodule Example do\n  defstruct [:\"display name\"]\nend\n";
    let fields = fields("example.ex", source);
    let field = fields
        .iter()
        .find(|node| node.name == "display name")
        .unwrap();
    let (start, end) = identifier_range(field);
    assert_eq!(start, source.find("display name").unwrap());
    assert_eq!(&source[start..end], "display name");
}

#[test]
fn cpp_initializer_member_reference_cannot_replace_a_later_declarator() {
    let source = "struct Other { int second; }; struct Example { Other other; int first = other.second, second; };\n";
    let fields = fields("example.cpp", source);
    let field = fields
        .iter()
        .find(|node| node.qualified_name.as_deref() == Some("Example.second"))
        .unwrap();
    let (start, end) = identifier_range(field);
    assert_eq!(start, source.rfind("second;").unwrap());
    assert_eq!(&source[start..end], "second");
}
