namespace Ecommerce.Correctness;

/// <summary>
/// Intentional static call graph for rgctl expected-facts checks.
/// Prefer static calls so extraction does not depend on DI.
/// </summary>
public static class CorrectnessHarness
{
    /// <summary>Leaf — no outbound application calls.</summary>
    public static int CorrectnessLeaf() => 42;

    /// <summary>Mid — calls <see cref="CorrectnessLeaf"/>.</summary>
    public static int CorrectnessMid() => CorrectnessLeaf() + 1;

    /// <summary>Root — calls <see cref="CorrectnessMid"/> and branches for CFG.</summary>
    public static int CorrectnessRoot(bool flag)
    {
        var value = CorrectnessMid();
        if (flag)
        {
            return value * 2;
        }
        return value;
    }

    public static int V2FreeHelper(int offset) => offset + 5;

    public static int V2StringTarget(string label) => label.Length;

    public static int V2SliceFlowProbe(int seedInput)
    {
        var deadUnused = 999;
        var stepVal = seedInput + 1;
        var aliasVal = stepVal;
        var finalOut = aliasVal * 2;
        return finalOut;
    }

    public static int ArchiveChargebackAudit() => 7;

    public static int ReconcileChargebackLedger() => ArchiveChargebackAudit() + 1;

    public static int CliqueAlphaB() => 11;

    public static int CliqueAlphaA() => CliqueAlphaB() + 1;

    public static int CliqueBetaB() => 22;

    public static int CliqueBetaA() => CliqueBetaB() + 1;
}

public interface IV2AuditPolicy
{
    int AuditWeight();
}

public class V2BaseLedger
{
    public virtual int AuditWeight() => 1;
}

public class V2OrderLedger : V2BaseLedger, IV2AuditPolicy
{
    public int BalanceAmount;

    public V2OrderLedger(int initial)
    {
        BalanceAmount = initial;
    }

    public int ReadBalance() => CorrectnessHarness.V2FreeHelper(42) + BalanceAmount;

    public void ApplyAdjustment(int delta)
    {
        this.BalanceAmount = this.BalanceAmount + delta;
    }

    public static int V2Overload(int x) => CorrectnessHarness.V2FreeHelper(x);

    public static int V2Overload(string label) => CorrectnessHarness.V2StringTarget(label);

    public override int AuditWeight() => ReadBalance();
}

public static class V2CloneCarrierA
{
    public static int V2CloneExact(int x)
    {
        var step = x + 17;
        var scaled = step * 3;
        var adjusted = scaled - 5;
        return adjusted + 1;
    }
}

public static class V2CloneCarrierB
{
    public static int V2CloneExact(int x)
    {
        var step = x + 17;
        var scaled = step * 3;
        var adjusted = scaled - 5;
        return adjusted + 1;
    }
}
