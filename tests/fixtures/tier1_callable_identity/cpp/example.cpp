class Base { public: virtual int quote(int value) { return 11; } virtual int quote(double value) { return 21; } };
class Child : public Base { public: int quote(int value) override { return 31; } };
