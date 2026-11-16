func v2FreeHelper(x: Int) -> Int {
    return x + 1
}

func v2StringTarget(label: String) -> Int {
    return label.count
}

class CorrectnessHarness {
    func correctnessLeaf() -> Int {
        return 42
    }

    func correctnessMid(flag: Bool) -> Int {
        if flag {
            return correctnessLeaf()
        }
        return 0
    }

    func correctnessRoot(n: Int) -> Int {
        if n > 0 {
            return correctnessMid(flag: true)
        } else {
            return -1
        }
    }

    func correctnessShared() -> Int {
        return 7
    }

    func correctnessLeft() -> Int {
        return correctnessShared() + 1
    }

    func correctnessRight() -> Int {
        return correctnessShared() + 2
    }

    func correctnessDiamond() -> Int {
        return correctnessLeft() + correctnessRight()
    }

    func v2SliceFlowProbe(seedInput: Int) -> Int {
        let noiseVal = 999
        let stepVal = seedInput + 10
        let aliasVal = stepVal
        let finalOut = aliasVal * 2
        if noiseVal < 0 {
            return 0
        }
        return finalOut
    }

    func taintUnsafeExec() -> Int {
        let raw = readLine()
        sqlite3_exec(raw)
        return 1
    }

    func archiveChargebackAudit(amount: Int) -> Int {
        return amount
    }

    func reconcileChargebackLedger(amount: Int) -> Int {
        return archiveChargebackAudit(amount: amount)
    }
}

class CfgFeatures {
    func cfgSwitch(code: Int) -> Int {
        switch code {
        case 1:
            return 10
        case 2:
            return 20
        default:
            return 0
        }
    }

    func cfgForLoop(limit: Int) -> Int {
        var acc = 0
        for i in 0...limit {
            acc = acc + i
        }
        return acc
    }
}

class V2OrderLedger: V2BaseLedger, V2AuditPolicy {
    var balance: Int = 0

    func applyAdjustment(delta: Int) {
        self.balance = self.balance + delta
    }

    func readBalance() -> Int {
        return v2FreeHelper(x: self.balance)
    }

    override func auditWeight() -> Int {
        return self.readBalance()
    }

    func v2Overload(x: Int) -> Int {
        return v2FreeHelper(x: x)
    }

    func v2Overload(label: String) -> Int {
        return v2StringTarget(label: label)
    }
}

class V2CloneCarrierA {
    func v2CloneExact(a: Int, b: Int) -> Int {
        let sumVal = a + b
        let prodVal = a * b
        if sumVal > prodVal {
            return sumVal - prodVal
        }
        return prodVal - sumVal
    }
}

class CliqueAlpha {
    func cliqueAlphaC(x: Int) -> Int {
        return x + 30
    }

    func cliqueAlphaB(x: Int) -> Int {
        return cliqueAlphaC(x: x + 20)
    }

    func cliqueAlphaA(x: Int) -> Int {
        return cliqueAlphaB(x: x + 10)
    }
}
