class Base:
    def quote(self, value: int) -> int:
        return 11
class Child(Base):
    def quote(self, value: int) -> int:
        return 21
