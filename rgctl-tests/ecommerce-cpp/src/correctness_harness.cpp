// Intentional call graph for rgctl expected-facts checks.
// Avoid intermediate locals that some extractors mis-label as callers.
// Definitions only in this .cpp (no header prototypes) to avoid ambiguous symbols.

namespace ecommerce {
namespace correctness {

int correctnessLeaf() { return 42; }

int correctnessMid() { return correctnessLeaf() + 1; }

int correctnessRoot(bool flag) {
  if (flag) {
    return correctnessMid() * 2;
  }
  return correctnessMid();
}

int v2FreeHelper(int offset) { return offset + 5; }

int v2StringTarget(int len) { return len + 9; }

class V2BaseLedger {
 public:
  virtual ~V2BaseLedger() = default;
  virtual int auditWeight() { return 1; }
};

class V2OrderLedger : public V2BaseLedger {
 public:
  int balanceAmount;

  explicit V2OrderLedger(int initial) : balanceAmount(initial) {}

  int readBalance() { return v2FreeHelper(42) + balanceAmount; }

  void applyAdjustment(int delta) {
    this->balanceAmount = this->balanceAmount + delta;
  }

  static int v2Overload(int x) { return v2FreeHelper(x); }

  static int v2Overload(double ratio) {
    return v2StringTarget(static_cast<int>(ratio));
  }

  int auditWeight() override { return readBalance(); }
};

int v2SliceFlowProbe(int seedInput) {
  int deadUnused = 999;
  int stepVal = seedInput + 1;
  int aliasVal = stepVal;
  int finalOut = aliasVal * 2;
  (void)deadUnused;
  return finalOut;
}

struct V2CloneCarrierA {
  static int v2CloneExact(int x) {
    int step = x + 17;
    int scaled = step * 3;
    int adjusted = scaled - 5;
    return adjusted + 1;
  }
};

struct V2CloneCarrierB {
  static int v2CloneExact(int x) {
    int step = x + 17;
    int scaled = step * 3;
    int adjusted = scaled - 5;
    return adjusted + 1;
  }
};

int archiveChargebackAudit() { return 7; }

int reconcileChargebackLedger() { return archiveChargebackAudit() + 1; }

int cliqueAlphaB() { return 11; }

int cliqueAlphaA() { return cliqueAlphaB() + 1; }

int cliqueBetaB() { return 22; }

int cliqueBetaA() { return cliqueBetaB() + 1; }

}  // namespace correctness
}  // namespace ecommerce
