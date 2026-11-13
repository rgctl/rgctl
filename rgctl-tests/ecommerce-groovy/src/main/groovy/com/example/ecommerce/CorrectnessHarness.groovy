package com.example.ecommerce

interface V2AuditPolicy {
    int auditWeight()
}

class V2BaseLedger {
    int auditWeight() {
        return 1
    }
}

class V2OrderLedger extends V2BaseLedger implements V2AuditPolicy {
    int balance

    V2OrderLedger(int initial) {
        this.balance = initial
    }

    int readBalance() {
        return CorrectnessHarness.v2FreeHelper(42) + this.balance
    }

    void applyAdjustment(int delta) {
        this.balance = this.balance + delta
    }

    static int v2Overload(int x) {
        return CorrectnessHarness.v2FreeHelper(x)
    }

    static int v2Overload(String label) {
        return CorrectnessHarness.v2StringTarget(label)
    }

    @Override
    int auditWeight() {
        return readBalance()
    }
}

class V2CloneCarrierA {
    static int v2CloneExact(int x) {
        int step = x + 17
        int scaled = step * 3
        int adjusted = scaled - 5
        return adjusted + 1
    }
}

class V2CloneCarrierB {
    static int v2CloneExact(int x) {
        int step = x + 17
        int scaled = step * 3
        int adjusted = scaled - 5
        return adjusted + 1
    }
}

class CfgFeatures {
    static int cfgForIn(List<Integer> items) {
        int total = 0
        for (int item in items) {
            total = total + item
        }
        return total
    }

    static int cfgSwitch(int code) {
        switch (code) {
            case 1:
                return 10
            case 2:
                return 20
            default:
                return 0
        }
    }

    static int cfgTryCatch(int input) {
        try {
            if (input < 0) {
                throw new IllegalArgumentException("neg")
            }
            return input + 1
        } catch (Exception ex) {
            return -1
        }
    }
}

class CorrectnessHarness {
    static int correctnessLeaf() {
        return 42
    }

    static int correctnessMid() {
        return correctnessLeaf() + 1
    }

    static int correctnessRoot(boolean flag) {
        int value = correctnessMid()
        if (flag) {
            return value * 2
        }
        return value
    }

    static int correctnessShared() {
        return 1
    }

    static int correctnessLeft() {
        return correctnessShared()
    }

    static int correctnessRight() {
        return correctnessShared()
    }

    static int correctnessDiamond() {
        return correctnessLeft() + correctnessRight()
    }

    static int v2FreeHelper(int offset) {
        return offset + 5
    }

    static int v2StringTarget(String label) {
        return label.length()
    }

    static int v2SliceFlowProbe(int seedInput) {
        int deadUnused = 999
        int stepVal = seedInput + 1
        int aliasVal = stepVal
        int finalOut = aliasVal * 2
        return finalOut
    }

    static void taintUnsafeExec(def request) {
        String cmd = request.getParameter("cmd")
        Runtime.getRuntime().exec(cmd)
    }

    static int archiveChargebackAudit() {
        return 7
    }

    static int reconcileChargebackLedger() {
        return archiveChargebackAudit() + 1
    }

    static int cliqueAlphaB() {
        return 11
    }

    static int cliqueAlphaA() {
        return cliqueAlphaB() + 1
    }

    static int cliqueBetaB() {
        return 22
    }

    static int cliqueBetaA() {
        return cliqueBetaB() + 1
    }
}
