# Identical module bodies exercise content-keyed facts reuse.
def module_helper():
    return 1

def module_exercise():
    return module_helper()

def module_outer():
    def module_inner():
        return 2
    return module_inner()
