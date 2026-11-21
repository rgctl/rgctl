package target
func binding_helper() int {return 111}
func binding_parameter(binding_helper func() int) int {return binding_helper()}
func binding_direct() int {return binding_helper()}

func binding_local(callback func() int) int { binding_helper:=callback; return binding_helper() }

func binding_after(callback func() int) int { { binding_helper:=callback; binding_helper() }; return binding_helper() }
