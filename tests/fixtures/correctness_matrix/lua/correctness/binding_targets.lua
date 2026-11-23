function binding_helper() return 111 end
function binding_parameter(binding_helper) return binding_helper() end
function binding_direct() return binding_helper() end

function binding_local(callback) local binding_helper=callback; return binding_helper() end

function binding_after(callback) do local binding_helper=callback; binding_helper() end; return binding_helper() end
