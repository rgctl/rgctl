use std::time::Instant;
use rbuilder::analysis::{build_cfg_for_function, DominatorTree};

fn main() {
    // Linear chain: 1000 sequential statements
    let mut code = String::from("fn linear(mut x: i32) -> i32 {\n");
    for i in 0..1000 {
        code.push_str(&format!("    x += {i};\n"));
    }
    code.push_str("    x\n}\n");
    let cfg = build_cfg_for_function("rust", &code, "linear").unwrap();
    println!("blocks: {}", cfg.blocks.len());
    let start = Instant::now();
    let dom = DominatorTree::build(&cfg);
    let elapsed = start.elapsed();
    println!("linear 1000 blocks idom+DF: {:?}", elapsed);
    assert!(!dom.idom.is_empty());

    // Nested if: ~3000 blocks from 100 nested ifs
    let mut code2 = String::from("fn nested(mut x: i32) -> i32 {\n");
    for i in 0..100 {
        code2.push_str(&format!("    if x > {i} {{ x += {i}; }}\n"));
    }
    code2.push_str("    x\n}\n");
    let cfg2 = build_cfg_for_function("rust", &code2, "nested").unwrap();
    println!("blocks nested: {}", cfg2.blocks.len());
    let start2 = Instant::now();
    let _dom2 = DominatorTree::build(&cfg2);
    println!("nested 100-if idom+DF: {:?}", start2.elapsed());
}
