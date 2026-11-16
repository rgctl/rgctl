package com.example.ecommerce

interface V2AuditPolicy {
    fun auditWeight(): Int
}

open class V2BaseLedger {
    open fun auditWeight(): Int = 1
}

class V2OrderLedger(var balance: Int) : V2BaseLedger(), V2AuditPolicy {
    fun readBalance(): Int {
        return CorrectnessHarness.v2FreeHelper(42) + this.balance
    }

    fun applyAdjustment(delta: Int) {
        this.balance = this.balance + delta
    }

    fun v2Overload(x: Int): Int {
        return CorrectnessHarness.v2FreeHelper(x)
    }

    fun v2Overload(label: String): Int {
        return CorrectnessHarness.v2StringTarget(label)
    }

    override fun auditWeight(): Int {
        return readBalance()
    }
}

object V2CloneCarrierA {
    fun v2CloneExact(x: Int): Int {
        val step = x + 17
        val scaled = step * 3
        val adjusted = scaled - 5
        return adjusted + 1
    }
}

object V2CloneCarrierB {
    fun v2CloneExact(x: Int): Int {
        val step = x + 17
        val scaled = step * 3
        val adjusted = scaled - 5
        return adjusted + 1
    }
}

object CfgFeatures {
    fun cfgForIn(items: List<Int>): Int {
        var total = 0
        for (item in items) {
            total += item
        }
        return total
    }

    fun cfgWhen(code: Int): Int {
        return when (code) {
            1 -> 10
            2 -> 20
            else -> 0
        }
    }

    fun cfgTryCatch(input: Int): Int {
        return try {
            if (input < 0) {
                throw IllegalArgumentException("neg")
            }
            input + 1
        } catch (ex: Exception) {
            -1
        }
    }
}

object CorrectnessHarness {
    fun correctnessLeaf(): Int {
        return 42
    }

    fun correctnessMid(): Int {
        return correctnessLeaf() + 1
    }

    fun correctnessRoot(flag: Boolean): Int {
        val value = correctnessMid()
        if (flag) {
            return value * 2
        }
        return value
    }

    fun correctnessShared(): Int {
        return 1
    }

    fun correctnessLeft(): Int {
        return correctnessShared()
    }

    fun correctnessRight(): Int {
        return correctnessShared()
    }

    fun correctnessDiamond(): Int {
        return correctnessLeft() + correctnessRight()
    }

    fun v2FreeHelper(offset: Int): Int {
        return offset + 5
    }

    fun v2StringTarget(label: String): Int {
        return label.length
    }

    fun v2SliceFlowProbe(seedInput: Int): Int {
        val deadUnused = 999
        val stepVal = seedInput + 1
        val aliasVal = stepVal
        val finalOut = aliasVal * 2
        return finalOut + (deadUnused - deadUnused) * 0
    }

    fun taintUnsafeExec() {
        val cmd = readLine() ?: ""
        Runtime.getRuntime().exec(cmd)
    }

    fun archiveChargebackAudit(): Int {
        return 7
    }

    fun reconcileChargebackLedger(): Int {
        return archiveChargebackAudit() + 1
    }

    fun cliqueAlphaB(): Int {
        return 11
    }

    fun cliqueAlphaA(): Int {
        return cliqueAlphaB() + 1
    }

    fun cliqueBetaB(): Int {
        return 22
    }

    fun cliqueBetaA(): Int {
        return cliqueBetaB() + 1
    }
}
