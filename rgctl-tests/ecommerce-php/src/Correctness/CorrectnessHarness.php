<?php
namespace App\Correctness;

interface V2AuditPolicy
{
    public function auditWeight(): int;
}

class V2BaseLedger
{
    public function auditWeight(): int
    {
        return 1;
    }
}

class V2OrderLedger extends V2BaseLedger implements V2AuditPolicy
{
    public int $balance;

    public function __construct(int $initial)
    {
        $this->balance = $initial;
    }

    public function readBalance(): int
    {
        return CorrectnessHarness::v2FreeHelper(42) + $this->balance;
    }

    public function applyAdjustment(int $delta): void
    {
        $this->balance = $this->balance + $delta;
    }

    public function auditWeight(): int
    {
        return $this->readBalance();
    }
}

class V2CloneCarrierA
{
    public static function v2CloneExact(int $x): int
    {
        $step = $x + 17;
        $scaled = $step * 3;
        $adjusted = $scaled - 5;
        return $adjusted + 1;
    }
}

class V2CloneCarrierB
{
    public static function v2CloneExact(int $x): int
    {
        $step = $x + 17;
        $scaled = $step * 3;
        $adjusted = $scaled - 5;
        return $adjusted + 1;
    }
}

class CfgFeatures
{
    public static function cfgForeach(array $items): int
    {
        $total = 0;
        foreach ($items as $item) {
            $total = $total + $item;
        }
        return $total;
    }

    public static function cfgSwitch(int $code): int
    {
        switch ($code) {
            case 1:
                return 10;
            case 2:
                return 20;
            default:
                return 0;
        }
    }

    public static function cfgTryCatch(int $input): int
    {
        try {
            if ($input < 0) {
                throw new \RuntimeException('neg');
            }
            return $input + 1;
        } catch (\Throwable $ex) {
            return -1;
        }
    }
}

class CorrectnessHarness
{
    public static function correctnessLeaf(): int
    {
        return 42;
    }

    public static function correctnessMid(): int
    {
        return self::correctnessLeaf() + 1;
    }

    public static function correctnessRoot(bool $flag): int
    {
        $value = self::correctnessMid();
        if ($flag) {
            return $value * 2;
        }
        return $value;
    }

    public static function correctnessShared(): int
    {
        return 1;
    }

    public static function correctnessLeft(): int
    {
        return self::correctnessShared();
    }

    public static function correctnessRight(): int
    {
        return self::correctnessShared();
    }

    public static function correctnessDiamond(): int
    {
        return self::correctnessLeft() + self::correctnessRight();
    }

    public static function v2FreeHelper(int $offset): int
    {
        return $offset + 5;
    }

    public static function v2SliceFlowProbe(int $seedInput): int
    {
        $deadUnused = 999;
        $stepVal = $seedInput + 1;
        $aliasVal = $stepVal;
        $finalOut = $aliasVal * 2;
        return $finalOut;
    }

    public static function taintUnsafeShell(): void
    {
        $cmd = $_GET['cmd'];
        system($cmd);
    }

    public static function archiveChargebackAudit(): int
    {
        return 7;
    }

    public static function reconcileChargebackLedger(): int
    {
        return self::archiveChargebackAudit() + 1;
    }

    public static function cliqueAlphaB(): int
    {
        return 11;
    }

    public static function cliqueAlphaA(): int
    {
        return self::cliqueAlphaB() + 1;
    }

    public static function cliqueBetaB(): int
    {
        return 22;
    }

    public static function cliqueBetaA(): int
    {
        return self::cliqueBetaB() + 1;
    }
}
