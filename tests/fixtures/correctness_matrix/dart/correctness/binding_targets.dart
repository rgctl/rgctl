int binding_helper()=>111; int binding_parameter(int Function() binding_helper)=>binding_helper(); int binding_direct()=>binding_helper();
int binding_local(int Function() callback){ final binding_helper=callback; return binding_helper(); }

int binding_after(int Function() callback) { { final binding_helper=callback; binding_helper(); } return binding_helper(); }
