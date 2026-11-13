package correctness

// Intentional call graph for rgctl expected-facts checks.

// CorrectnessLeaf is the leaf — no outbound application calls.
func CorrectnessLeaf() int {
	return 42
}

// CorrectnessMid calls CorrectnessLeaf.
func CorrectnessMid() int {
	return CorrectnessLeaf() + 1
}

// CorrectnessRoot calls CorrectnessMid and branches for a non-trivial CFG.
func CorrectnessRoot(flag bool) int {
	value := CorrectnessMid()
	if flag {
		return value * 2
	}
	return value
}

func V2FreeHelper(offset int) int {
	return offset + 5
}

type V2AuditPolicy interface {
	AuditWeight() int
}

type V2BaseLedger struct {
	Code int
}

type V2OrderLedger struct {
	V2BaseLedger
	BalanceAmount int
}

func NewV2OrderLedger(initial int) *V2OrderLedger {
	return &V2OrderLedger{BalanceAmount: initial}
}

func (l *V2OrderLedger) ReadBalance() int {
	return V2FreeHelper(42) + l.BalanceAmount
}

func (l *V2OrderLedger) ApplyAdjustment(delta int) {
	l.BalanceAmount = l.BalanceAmount + delta
}

func (l *V2OrderLedger) AuditWeight() int {
	return l.ReadBalance()
}

type V2AltLedger struct{}

func (a *V2AltLedger) AuditWeight() int {
	return 1
}

func V2SliceFlowProbe(seedInput int) int {
	deadUnused := 999
	_ = deadUnused
	stepVal := seedInput + 1
	aliasVal := stepVal
	finalOut := aliasVal * 2
	return finalOut
}

func V2CloneBloomA(amountCents int, taxRateBps int, discountBps int) int {
	grossTotal := amountCents + (amountCents*taxRateBps)/10000
	discountTotal := (grossTotal * discountBps) / 10000
	netPayable := grossTotal - discountTotal
	if netPayable < 0 {
		return 0
	}
	return netPayable + V2FreeHelper(1)
}

func V2CloneBloomB(amountCents int, taxRateBps int, discountBps int) int {
	grossTotal := amountCents + (amountCents*taxRateBps)/10000
	discountTotal := (grossTotal * discountBps) / 10000
	netPayable := grossTotal - discountTotal
	if netPayable < 0 {
		return 0
	}
	return netPayable + V2FreeHelper(1)
}

func ArchiveChargebackAudit() int {
	return 7
}

func ReconcileChargebackLedger() int {
	return ArchiveChargebackAudit() + 1
}

func CliqueAlphaB() int {
	return 11
}

func CliqueAlphaA() int {
	return CliqueAlphaB() + 1
}

func CliqueBetaB() int {
	return 22
}

func CliqueBetaA() int {
	return CliqueBetaB() + 1
}
