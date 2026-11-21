function binding_helper() { 111 } function binding_parameter(String $binding_helper) { call($binding_helper) } function binding_direct(String $binding_helper) { binding_helper() }
function binding_local(String $callback) { $binding_helper=$callback; call($binding_helper) }
