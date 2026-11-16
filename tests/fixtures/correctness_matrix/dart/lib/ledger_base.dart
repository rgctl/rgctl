abstract interface class V2AuditPolicy {
  int auditWeight();
}

class V2BaseLedger {
  int auditWeight() {
    return 1;
  }
}

class V2CloneCarrierB {
  int v2CloneExact(int a, int b) {
    final sumVal = a + b;
    final prodVal = a * b;
    if (sumVal > prodVal) {
      return sumVal - prodVal;
    }
    return prodVal - sumVal;
  }
}

class CliqueBeta {
  int cliqueBetaC(int x) {
    return x + 3;
  }

  int cliqueBetaB(int x) {
    return cliqueBetaC(x + 2);
  }

  int cliqueBetaA(int x) {
    return cliqueBetaB(x + 1);
  }
}
