fn binding_helper()->i32{111} fn binding_parameter(binding_helper:fn()->i32)->i32{binding_helper()} fn binding_direct()->i32{binding_helper()}
fn binding_local(callback:fn()->i32)->i32{let binding_helper=callback;binding_helper()}

fn binding_after(callback:fn()->i32)->i32 { { let binding_helper=callback; binding_helper(); } binding_helper() }
fn binding_non_capture(binding_helper:fn()->i32)->i32 { fn binding_inner()->i32 { binding_helper() } binding_inner() }
