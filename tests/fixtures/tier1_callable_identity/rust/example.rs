trait One { fn quote(&self) -> i32; }
trait Two { fn quote(&self) -> i32; }
struct Value;
impl One for Value { fn quote(&self) -> i32 { return 11; } }
impl Two for Value { fn quote(&self) -> i32 { return 21; } }
