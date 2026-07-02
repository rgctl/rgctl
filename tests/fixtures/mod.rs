//! Semantic audit fixture code samples.

pub mod dead_code_post_return {
    pub const CODE: &str = r#"
fn dead_after_return() -> i32 {
    return 1;
    let unreachable = 99;
    unreachable
}
"#;
    pub const FN: &str = "dead_after_return";
}

pub mod diamond_merge {
    pub const CODE: &str = r#"
fn diamond(x: i32) -> i32 {
    let mut y = 0;
    if x > 0 {
        y = x + 1;
    } else {
        y = x - 1;
    }
    y + 1
}
"#;
    pub const FN: &str = "diamond";
}

pub mod loop_back_edge {
    pub const CODE: &str = r#"
fn sum(n: i32) -> i32 {
    let mut s = 0;
    let mut i = 0;
    while i < n {
        s += i;
        i += 1;
    }
    s
}
"#;
    pub const FN: &str = "sum";
}

pub mod sanitizer_bypass {
    pub const CODE: &str = r#"
def handle(request):
    user = request.GET['user']
    if len(user) > 0:
        user = int(user)
    cursor.execute(user)
"#;
    pub const FN: &str = "handle";
}

pub mod interprocedural_handoff {
    pub const SOURCE: &str = r#"
fn main() {
    let data = read_input();
    let result = process(data);
    write_output(result);
}
fn process(input: String) -> String {
    let trimmed = input.trim();
    format!("Processed: {}", trimmed)
}
fn read_input() -> String { String::new() }
fn write_output(_: String) {}
"#;
}
