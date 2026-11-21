object BindingTargets { def binding_helper():Int = 111; def binding_parameter(binding_helper:()=>Int):Int = binding_helper(); def binding_direct():Int = binding_helper() def binding_local(callback:()=>Int):Int = { val binding_helper=callback; binding_helper() } 
def binding_after(callback:()=>Int):Int = { if(true) { val binding_helper=callback; binding_helper() }; binding_helper() }
}