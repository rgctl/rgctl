/**
 * Intentional call graph for rgctl expected-facts checks.
 * Prefer direct function calls so extraction does not depend on DI.
 */

function correctnessLeaf() {
  return 42;
}

function correctnessMid() {
  return correctnessLeaf() + 1;
}

function correctnessRoot(flag) {
  const value = correctnessMid();
  if (flag) {
    return value * 2;
  }
  return value;
}

function v2FreeHelper(offset) {
  return offset + 5;
}

class V2BaseLedger {
  auditWeight() {
    return 1;
  }
}

class V2OrderLedger extends V2BaseLedger {
  constructor(initial) {
    super();
    this.balanceAmount = initial;
  }

  readBalance() {
    return v2FreeHelper(42) + this.balanceAmount;
  }

  applyAdjustment(delta) {
    this.balanceAmount = this.balanceAmount + delta;
  }

  auditWeight() {
    return this.readBalance();
  }
}

function v2SliceFlowProbe(seedInput) {
  const deadUnused = 999;
  const stepVal = seedInput + 1;
  const aliasVal = stepVal;
  const finalOut = aliasVal * 2;
  return finalOut;
}

class V2CloneCarrierA {
  static v2CloneExact(x) {
    const step = x + 17;
    const scaled = step * 3;
    const adjusted = scaled - 5;
    return adjusted + 1;
  }
}

class V2CloneCarrierB {
  static v2CloneExact(x) {
    const step = x + 17;
    const scaled = step * 3;
    const adjusted = scaled - 5;
    return adjusted + 1;
  }
}

function archiveChargebackAudit() {
  return 7;
}

function reconcileChargebackLedger() {
  return archiveChargebackAudit() + 1;
}

function cliqueAlphaB() {
  return 11;
}

function cliqueAlphaA() {
  return cliqueAlphaB() + 1;
}

function cliqueBetaB() {
  return 22;
}

function cliqueBetaA() {
  return cliqueBetaB() + 1;
}

module.exports = { correctnessLeaf, correctnessMid, correctnessRoot, V2OrderLedger, v2SliceFlowProbe, reconcileChargebackLedger };
