<?php
function operand_sink($value) { return $value; }
function operand_relay($value) { $alias = $value; return operand_sink($alias); }

function operand_member($value, $other) { return $value->perform($other); }

class OperandSelf { public function operand_instance($other) { return $this->perform($other); } }


function operand_choice($flag, $left, $right) { return $flag ? $left : $right; }
