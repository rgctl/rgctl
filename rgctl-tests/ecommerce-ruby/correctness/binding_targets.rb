def binding_helper; 111; end
def binding_parameter(binding_helper); binding_helper.call; end
def binding_direct(binding_helper); binding_helper(); end

def binding_local(callback); binding_helper=callback; binding_helper.call; end
