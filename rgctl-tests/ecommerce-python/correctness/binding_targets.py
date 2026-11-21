def binding_helper(): return 111
def binding_parameter(binding_helper): return binding_helper()
def binding_direct(): return binding_helper()

def binding_local(callback):
    binding_helper=callback
    return binding_helper()


def binding_after(callback):
    if True:
        binding_helper=callback
        binding_helper()
    return binding_helper()

