function binding_helper():number{return 111;} function binding_parameter(binding_helper:()=>number):number{return binding_helper();} function binding_direct():number{return binding_helper();}
function binding_local(callback:()=>number):number { const binding_helper=callback; return binding_helper(); }

function binding_after(callback:()=>number):number { { let binding_helper=callback; binding_helper(); } return binding_helper(); }
