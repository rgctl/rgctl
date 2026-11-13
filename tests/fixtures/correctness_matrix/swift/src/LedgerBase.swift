protocol V2AuditPolicy {
    func auditWeight() -> Int
}

class V2BaseLedger {
    func auditWeight() -> Int {
        return 1
    }
}

class V2CloneCarrierB {
    func v2CloneExact(a: Int, b: Int) -> Int {
        let sumVal = a + b
        let prodVal = a * b
        if sumVal > prodVal {
            return sumVal - prodVal
        }
        return prodVal - sumVal
    }
}

class CliqueBeta {
    func cliqueBetaC(x: Int) -> Int {
        return x + 3
    }

    func cliqueBetaB(x: Int) -> Int {
        return cliqueBetaC(x: x + 2)
    }

    func cliqueBetaA(x: Int) -> Int {
        return cliqueBetaB(x: x + 1)
    }
}
