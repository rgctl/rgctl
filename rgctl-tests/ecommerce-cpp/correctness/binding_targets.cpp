int binding_helper(){return 111;} int binding_parameter(int (*binding_helper)()){return binding_helper();} int binding_direct(){return binding_helper();}
int binding_local(int (*callback)()){ auto binding_helper=callback; return binding_helper(); }

int binding_after(int (*callback)()) { { auto binding_helper=callback; binding_helper(); } return binding_helper(); }
