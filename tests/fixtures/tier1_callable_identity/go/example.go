package example
type Base struct {}
type Child struct { Base }
func (b Base) Quote(value int) int { return 11 }
func (c Child) Quote(value int) int { return 21 }
