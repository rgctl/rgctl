function binding_helper(){return 111;} function binding_parameter(binding_helper){return binding_helper();} function binding_direct(){return binding_helper();}
function binding_local(callback){ const binding_helper=callback; return binding_helper(); }

function binding_after(callback) { { let binding_helper=callback; binding_helper(); } return binding_helper(); }
