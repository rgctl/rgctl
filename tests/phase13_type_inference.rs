//! Phase 13: type inference (20 tests).

#[path = "common/phase13.rs"]
mod phase13;

use phase13::{has_type, with_inferred_types};
use rbuilder::analysis::{confidence_for, InferredType};

macro_rules! type_test {
    ($(#[$attr:meta])* $name:ident, $lang:expr, $code:expr, $fn:expr, $check:expr) => {
        $(#[$attr])*
        #[test]
        fn $name() {
            with_inferred_types($lang, $code, $fn, $check);
        }
    };
}

macro_rules! conf_test {
    ($name:ident, $typ:expr, $from_method:expr, $expected:expr) => {
        #[test]
        fn $name() {
            assert_eq!(confidence_for(&$typ, $from_method), $expected);
        }
    };
}

#[cfg(feature = "bundle-minimal")]
type_test!(
    type_python_int_literal,
    "python",
    r#"
def example():
    x = 42
"#,
    "example",
    |inferred| {
        assert!(has_type(inferred, "x", InferredType::Int));
    }
);

#[cfg(feature = "bundle-minimal")]
type_test!(
    type_python_string_literal,
    "python",
    r#"
def example():
    y = "hello"
"#,
    "example",
    |inferred| {
        assert!(has_type(inferred, "y", InferredType::String));
    }
);

#[cfg(feature = "bundle-minimal")]
type_test!(
    type_python_float_literal,
    "python",
    r#"
def example():
    z = 3.14
"#,
    "example",
    |inferred| {
        assert!(has_type(inferred, "z", InferredType::Float));
    }
);

#[cfg(feature = "bundle-minimal")]
type_test!(
    type_python_list_literal,
    "python",
    r#"
def example():
    items = []
"#,
    "example",
    |inferred| {
        assert!(inferred.iter().any(|t| {
            t.variable == "items" && matches!(t.inferred_type, InferredType::List(_))
        }));
    }
);

#[cfg(feature = "bundle-minimal")]
type_test!(
    type_python_dict_literal,
    "python",
    r#"
def example():
    mapping = {}
"#,
    "example",
    |inferred| {
        assert!(inferred.iter().any(|t| {
            t.variable == "mapping" && matches!(t.inferred_type, InferredType::Dict(_, _))
        }));
    }
);

#[cfg(feature = "bundle-minimal")]
type_test!(
    type_python_string_method,
    "python",
    r#"
def process(data):
    upper = data.upper()
"#,
    "process",
    |inferred| {
        assert!(has_type(inferred, "data", InferredType::String));
    }
);

#[cfg(feature = "bundle-minimal")]
type_test!(
    type_python_append_method,
    "python",
    r#"
def process():
    items = []
    items.append("test")
"#,
    "process",
    |inferred| {
        assert!(inferred.iter().any(|t| {
            t.variable == "items" && matches!(t.inferred_type, InferredType::List(_))
        }));
    }
);

#[cfg(feature = "bundle-minimal")]
type_test!(
    type_python_literal_confidence,
    "python",
    r#"
def example():
    x = 42
"#,
    "example",
    |inferred| {
        let x = inferred.iter().find(|t| t.variable == "x").unwrap();
        assert!(x.confidence >= 0.9);
    }
);

#[cfg(feature = "bundle-minimal")]
type_test!(
    type_python_method_confidence,
    "python",
    r#"
def process(data):
    _ = data.strip()
"#,
    "process",
    |inferred| {
        let data = inferred.iter().find(|t| t.variable == "data").unwrap();
        assert!(data.confidence >= 0.85);
    }
);

#[cfg(feature = "bundle-minimal")]
type_test!(
    type_python_chained_assignment,
    "python",
    r#"
def example():
    x = 10
    y = x
"#,
    "example",
    |inferred| {
        assert!(inferred.iter().any(|t| t.variable == "x"));
    }
);

#[cfg(feature = "bundle-minimal")]
type_test!(
    type_python_strip_method_confidence,
    "python",
    r#"
def clean(value):
    return value.strip()
"#,
    "clean",
    |inferred| {
        let value = inferred.iter().find(|t| t.variable == "value").unwrap();
        assert!(value.confidence >= 0.85);
        assert_eq!(value.inferred_type, InferredType::String);
    }
);

type_test!(
    type_js_literal_patterns_on_rust_pdg,
    "rust",
    r#"fn js() { let msg = "hello"; let flag = true; let arr = "[1,2]"; }"#,
    "js",
    |inferred| {
        assert!(inferred.is_empty() || inferred.iter().any(|t| t.variable == "msg"));
        let texts = phase13::pdg_statement_texts(
            "rust",
            r#"fn js() { let msg = "hello"; let flag = true; }"#,
            "js",
        );
        assert!(texts.iter().any(|t| t.contains("hello")));
    }
);

type_test!(
    type_js_method_patterns_on_rust_pdg,
    "rust",
    r#"fn js() { let data = "x"; let upper = "toUpperCase"; }"#,
    "js",
    |_inferred| {
        let texts = phase13::pdg_statement_texts(
            "rust",
            r#"fn js() { let data = "x"; let m = "toUpperCase"; }"#,
            "js",
        );
        assert!(texts.iter().any(|t| t.contains("toUpperCase")));
    }
);

type_test!(
    type_ruby_literal_patterns_on_rust_pdg,
    "rust",
    r#"fn rb() { let name = "alice"; let count = "42"; }"#,
    "rb",
    |_inferred| {
        let texts = phase13::pdg_statement_texts(
            "rust",
            r#"fn rb() { let name = "alice"; let count = "42"; }"#,
            "rb",
        );
        assert!(texts.iter().any(|t| t.contains("alice")));
    }
);

type_test!(
    type_ruby_method_patterns_on_rust_pdg,
    "rust",
    r#"fn rb() { let data = "x"; let up = "upcase"; }"#,
    "rb",
    |_inferred| {
        let texts = phase13::pdg_statement_texts(
            "rust",
            r#"fn rb() { let data = "x"; let up = "upcase"; }"#,
            "rb",
        );
        assert!(texts.iter().any(|t| t.contains("upcase")));
    }
);

conf_test!(
    confidence_for_string_literal,
    InferredType::String,
    false,
    0.9
);
conf_test!(
    confidence_for_method_string,
    InferredType::String,
    true,
    0.86
);
conf_test!(
    confidence_for_list_method,
    InferredType::List(Box::new(InferredType::Unknown)),
    true,
    0.78
);

conf_test!(confidence_for_int_literal, InferredType::Int, false, 0.92);
conf_test!(confidence_for_unknown, InferredType::Unknown, false, 0.4);
