/**
 * Intentional call graph for rgctl expected-facts checks.
 * Prefer direct function calls so extraction does not depend on DI.
 */

export function correctnessLeaf(): number {
  return 42;
}

export function correctnessMid(): number {
  return correctnessLeaf() + 1;
}

export function correctnessRoot(flag: boolean): number {
  const value = correctnessMid();
  if (flag) {
    return value * 2;
  }
  return value;
}

export function v2FreeHelper(offset: number): number {
  return offset + 5;
}

export interface V2AuditPolicy {
  auditWeight(): number;
}

export class V2BaseLedger {
  auditWeight(): number {
    return 1;
  }
}

export class V2OrderLedger extends V2BaseLedger implements V2AuditPolicy {
  balanceAmount: number;

  constructor(initial: number) {
    super();
    this.balanceAmount = initial;
  }

  readBalance(): number {
    return v2FreeHelper(42) + this.balanceAmount;
  }

  applyAdjustment(delta: number): void {
    this.balanceAmount = this.balanceAmount + delta;
  }

  override auditWeight(): number {
    return this.readBalance();
  }
}

export function v2SliceFlowProbe(seedInput: number): number {
  const deadUnused = 999;
  const stepVal = seedInput + 1;
  const aliasVal = stepVal;
  const finalOut = aliasVal * 2;
  void deadUnused;
  return finalOut;
}

export class V2CloneCarrierA {
  static v2CloneExact(x: number): number {
    const step = x + 17;
    const scaled = step * 3;
    const adjusted = scaled - 5;
    return adjusted + 1;
  }
}

export class V2CloneCarrierB {
  static v2CloneExact(x: number): number {
    const step = x + 17;
    const scaled = step * 3;
    const adjusted = scaled - 5;
    return adjusted + 1;
  }
}

export function archiveChargebackAudit(): number {
  return 7;
}

export function reconcileChargebackLedger(): number {
  return archiveChargebackAudit() + 1;
}

export function cliqueAlphaB(): number {
  return 11;
}

export function cliqueAlphaA(): number {
  return cliqueAlphaB() + 1;
}

export function cliqueBetaB(): number {
  return 22;
}

export function cliqueBetaA(): number {
  return cliqueBetaB() + 1;
}
