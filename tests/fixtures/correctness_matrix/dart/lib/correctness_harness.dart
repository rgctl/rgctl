import 'ledger_base.dart';

int v2FreeHelper(int x) {
  return x + 1;
}

class CorrectnessHarness {
  int correctnessLeaf() {
    return 42;
  }

  int correctnessMid(bool flag) {
    if (flag) {
      return correctnessLeaf();
    }
    return 0;
  }

  int correctnessRoot(int n) {
    if (n > 0) {
      return correctnessMid(true);
    } else {
      return -1;
    }
  }

  int correctnessShared() {
    return 7;
  }

  int correctnessLeft() {
    return correctnessShared() + 1;
  }

  int correctnessRight() {
    return correctnessShared() + 2;
  }

  int correctnessDiamond() {
    return correctnessLeft() + correctnessRight();
  }

  int v2SliceFlowProbe(int seedInput) {
    final noiseVal = 999;
    final stepVal = seedInput + 10;
    final aliasVal = stepVal;
    final finalOut = aliasVal * 2;
    if (noiseVal < 0) {
      return 0;
    }
    return finalOut;
  }

  int taintUnsafeExec() {
    final raw = stdin.readLineSync();
    rawQuery(raw);
    return 1;
  }

  int archiveChargebackAudit(int amount) {
    return amount;
  }

  int reconcileChargebackLedger(int amount) {
    return archiveChargebackAudit(amount);
  }
}

class CfgFeatures {
  int cfgSwitch(int code) {
    switch (code) {
      case 1:
        return 10;
      case 2:
        return 20;
      default:
        return 0;
    }
  }

  int cfgForLoop(int limit) {
    var acc = 0;
    for (var i = 0; i < limit; i++) {
      acc = acc + i;
    }
    return acc;
  }
}

class V2OrderLedger extends V2BaseLedger implements V2AuditPolicy {
  int readBalance(int b) {
    return v2FreeHelper(b);
  }

  int auditWeight() {
    return readBalance(10);
  }
}

class V2CloneCarrierA {
  int v2CloneExact(int a, int b) {
    final sumVal = a + b;
    final prodVal = a * b;
    if (sumVal > prodVal) {
      return sumVal - prodVal;
    }
    return prodVal - sumVal;
  }
}

class CliqueAlpha {
  int cliqueAlphaC(int x) {
    return x + 30;
  }

  int cliqueAlphaB(int x) {
    return cliqueAlphaC(x + 20);
  }

  int cliqueAlphaA(int x) {
    return cliqueAlphaB(x + 10);
  }
}
