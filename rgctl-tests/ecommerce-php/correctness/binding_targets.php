<?php function binding_helper(){return 111;} function binding_parameter($binding_helper){return $binding_helper();} function binding_direct($binding_helper){return binding_helper();}
function binding_local($callback){ $binding_helper=$callback; return $binding_helper(); }
