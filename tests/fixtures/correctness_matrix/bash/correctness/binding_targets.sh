binding_helper() { printf 111; }
binding_parameter() { local binding_helper=$1; "$binding_helper"; }
binding_direct() { local binding_helper=ignored; binding_helper; }

binding_local() { local binding_helper=$1; "$binding_helper"; }
