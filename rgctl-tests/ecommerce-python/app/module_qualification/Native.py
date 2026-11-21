class Native:
    def __init__(self):
        self.value = 1
    @staticmethod
    def create():
        return 1
    def method(self):
        return outside()
def create():
    return 2
def exercise():
    return create()
def outside():
    return 3
