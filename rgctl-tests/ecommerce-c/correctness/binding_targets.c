int binding_helper(void){return 111;} int binding_parameter(int (*binding_helper)(void)){return binding_helper();} int binding_direct(void){return binding_helper();}
int binding_local(int (*callback)(void)){ int (*binding_helper)(void)=callback; return binding_helper(); }

int binding_after(int (*callback)(void)) { { int (*binding_helper)(void)=callback; binding_helper(); } return binding_helper(); }
