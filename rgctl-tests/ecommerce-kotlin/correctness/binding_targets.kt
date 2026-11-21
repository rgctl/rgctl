fun binding_helper():Int = 111
fun binding_parameter(binding_helper:()->Int):Int = binding_helper()
fun binding_direct():Int = binding_helper()

fun binding_local(callback:()->Int):Int { val binding_helper=callback; return binding_helper() }

fun binding_after(callback:()->Int):Int { if(true) { val binding_helper=callback; binding_helper() }; return binding_helper() }
