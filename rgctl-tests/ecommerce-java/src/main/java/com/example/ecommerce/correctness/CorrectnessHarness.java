package com.example.ecommerce.correctness;

/**
 * Intentional static call graph for rgctl expected-facts checks.
 *
 * <p>Keep method names unique and stable. Prefer static calls so extraction does not
 * depend on Spring DI field resolution.
 */
public final class CorrectnessHarness {

    private CorrectnessHarness() {}

    /** Leaf — no outbound application calls. */
    public static int correctnessLeaf() {
        return 42;
    }

    /** Mid — calls {@link #correctnessLeaf()}. */
    public static int correctnessMid() {
        return correctnessLeaf() + 1;
    }

    /**
     * Root — calls {@link #correctnessMid()} and branches for a non-trivial CFG.
     *
     * @param flag branch selector
     * @return transformed mid value
     */
    public static int correctnessRoot(boolean flag) {
        int value = correctnessMid();
        if (flag) {
            return value * 2;
        }
        return value;
    }

    /** Shared sink for diamond topology QE. */
    public static int correctnessShared() {
        return 1;
    }

    public static int correctnessLeft() {
        return correctnessShared();
    }

    public static int correctnessRight() {
        return correctnessShared();
    }

    /** Diamond root — dual callers into {@link #correctnessShared()}. */
    public static int correctnessDiamond() {
        return correctnessLeft() + correctnessRight();
    }

    public static int v2FreeHelper(int offset) {
        return offset + 5;
    }

    public static int v2StringTarget(String label) {
        return label.length();
    }

    public static int v2SliceFlowProbe(int seedInput) {
        int deadUnused = 999;
        int stepVal = seedInput + 1;
        int aliasVal = stepVal;
        int finalOut = aliasVal * 2;
        return finalOut;
    }

    public static int archiveChargebackAudit() {
        return 7;
    }

    public static int reconcileChargebackLedger() {
        return archiveChargebackAudit() + 1;
    }

    public static int cliqueAlphaB() {
        return 11;
    }

    public static int cliqueAlphaA() {
        return cliqueAlphaB() + 1;
    }

    public static int cliqueBetaB() {
        return 22;
    }

    public static int cliqueBetaA() {
        return cliqueBetaB() + 1;
    }
}

interface V2AuditPolicy {
    default int auditWeight() {
        return 1;
    }
}

class V2BaseLedger {
    protected int baseCode() {
        return 10;
    }
}

class V2OrderLedger extends V2BaseLedger implements V2AuditPolicy {
    public int balance;

    public V2OrderLedger(int balance) {
        this.balance = balance;
    }

    public int balance() {
        return CorrectnessHarness.v2FreeHelper(42) + this.balance;
    }

    public void applyAdjustment(int delta) {
        this.balance = this.balance + delta;
    }

    public static int v2Overload(int x) {
        return CorrectnessHarness.v2FreeHelper(x);
    }

    public static int v2Overload(String label) {
        return CorrectnessHarness.v2StringTarget(label);
    }

    @Override
    public int auditWeight() {
        return this.balance();
    }
}

class V2CloneCarrierA {
    public static int v2CloneExact(int x) {
        int step = x + 17;
        int scaled = step * 3;
        int adjusted = scaled - 5;
        return adjusted + 1;
    }
}

class V2CloneCarrierB {
    public static int v2CloneExact(int x) {
        int step = x + 17;
        int scaled = step * 3;
        int adjusted = scaled - 5;
        return adjusted + 1;
    }
}
