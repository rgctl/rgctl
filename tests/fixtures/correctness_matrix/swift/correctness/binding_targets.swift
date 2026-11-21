func binding_helper()->Int{return 111}
func binding_parameter(_ binding_helper:()->Int)->Int{return binding_helper()}
func binding_direct()->Int{return binding_helper()}

func binding_local(_ callback:()->Int)->Int { let binding_helper=callback; return binding_helper() }

func binding_after(_ callback:()->Int)->Int { if true { let binding_helper=callback; _=binding_helper() }; return binding_helper() }
