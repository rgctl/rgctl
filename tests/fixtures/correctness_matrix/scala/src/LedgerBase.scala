trait V2AuditPolicy {
  def auditWeight(): Int
}

class V2BaseLedger {
  def auditWeight(): Int = {
    return 1
  }
}

class V2CloneCarrierB {
  def v2CloneExact(a: Int, b: Int): Int = {
    val sumVal = a + b
    val prodVal = a * b
    if (sumVal > prodVal) {
      return sumVal - prodVal
    }
    return prodVal - sumVal
  }
}

class CliqueBeta {
  def cliqueBetaC(x: Int): Int = {
    return x + 3
  }

  def cliqueBetaB(x: Int): Int = {
    return cliqueBetaC(x + 2)
  }

  def cliqueBetaA(x: Int): Int = {
    return cliqueBetaB(x + 1)
  }
}
