class CorrectnessHelpers {
  def v2FreeHelper(x: Int): Int = {
    return x + 1
  }

  def v2StringTarget(label: String): Int = {
    return label.length
  }
}

class CorrectnessHarness {
  def correctnessLeaf(): Int = {
    return 42
  }

  def correctnessMid(flag: Boolean): Int = {
    if (flag) {
      return correctnessLeaf()
    }
    return 0
  }

  def correctnessRoot(n: Int): Int = {
    if (n > 0) {
      return correctnessMid(true)
    } else {
      return -1
    }
  }

  def correctnessShared(): Int = {
    return 7
  }

  def correctnessLeft(): Int = {
    return correctnessShared() + 1
  }

  def correctnessRight(): Int = {
    return correctnessShared() + 2
  }

  def correctnessDiamond(): Int = {
    return correctnessLeft() + correctnessRight()
  }

  def v2SliceFlowProbe(seedInput: Int): Int = {
    val noiseVal = 999
    val stepVal = seedInput + 10
    val aliasVal = stepVal
    val finalOut = aliasVal * 2
    if (noiseVal < 0) {
      return 0
    }
    return finalOut
  }

  def taintUnsafeExec(): Int = {
    val raw = StdIn.readLine()
    executeQuery(raw)
    return 1
  }

  def archiveChargebackAudit(amount: Int): Int = {
    return amount
  }

  def reconcileChargebackLedger(amount: Int): Int = {
    return archiveChargebackAudit(amount)
  }
}

class CfgFeatures {
  def cfgMatch(code: Int): Int = {
    code match {
      case 1 => 10
      case 2 => 20
      case _ => 0
    }
  }

  def cfgWhileLoop(limit: Int): Int = {
    var acc = 0
    while (acc < limit) {
      acc = acc + 1
    }
    return acc
  }
}

class V2OrderLedger extends V2BaseLedger with V2AuditPolicy {
  var balance: Int = 0

  def applyAdjustment(delta: Int): Unit = {
    this.balance = this.balance + delta
  }

  def readBalance(helpers: CorrectnessHelpers): Int = {
    return helpers.v2FreeHelper(this.balance)
  }

  override def auditWeight(): Int = {
    val helpers = new CorrectnessHelpers()
    return this.readBalance(helpers)
  }

  def v2Overload(x: Int): Int = {
    val helpers = new CorrectnessHelpers()
    return helpers.v2FreeHelper(x)
  }

  def v2Overload(label: String): Int = {
    val helpers = new CorrectnessHelpers()
    return helpers.v2StringTarget(label)
  }
}

class V2CloneCarrierA {
  def v2CloneExact(a: Int, b: Int): Int = {
    val sumVal = a + b
    val prodVal = a * b
    if (sumVal > prodVal) {
      return sumVal - prodVal
    }
    return prodVal - sumVal
  }
}

class CliqueAlpha {
  def cliqueAlphaC(x: Int): Int = {
    return x + 30
  }

  def cliqueAlphaB(x: Int): Int = {
    return cliqueAlphaC(x + 20)
  }

  def cliqueAlphaA(x: Int): Int = {
    return cliqueAlphaB(x + 10)
  }
}
