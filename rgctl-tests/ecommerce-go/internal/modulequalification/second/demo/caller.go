package demo

type ShadowContainer[PackageHelper any] struct{ Value PackageHelper }

func PackageRun() int { return PackageHelper() }

func PackageParameter(PackageHelper func() int) int { return PackageHelper() }

func PackageLocal() int {
	PackageHelper := func() int { return 2 }
	return PackageHelper()
}

func PackageTyped() int {
	var PackageHelper func() int = func() int { return 2 }
	return PackageHelper()
}

func PackageBoundaries() int {
	result := PackageHelper()
	{
		PackageHelper := func() int { return 2 }
		result += PackageHelper()
	}
	return result + PackageHelper()
}

func PackageNestedType(callback func(PackageHelper int)) int { return PackageHelper() }

func PackageRange(values []func() int) int {
	for _, PackageHelper := range values {
		_ = PackageHelper()
	}
	return PackageHelper()
}

func PackageSelect(values chan func() int) int {
	select {
	case PackageHelper := <-values:
		return PackageHelper()
	default:
		return PackageHelper()
	}
}

func PackageCondition() int {
	if PackageHelper := func() int { return 2 }; true {
		return PackageHelper()
	}
	return PackageHelper()
}

func PackageTypeSwitch(value any) int {
	switch PackageHelper := value.(type) {
	case func() int:
		return PackageHelper()
	}
	return PackageHelper()
}

func PackageLiteral() int {
	return func(PackageHelper func() int) int { return PackageHelper() }(func() int { return 2 })
}

func PackageParen(PackageHelper func() int) int { return (PackageHelper)() }
func PackageParenDirect() int                   { return (PackageHelper)() }
