<?php
function operand_sink($value) { return $value; }
function operand_relay($value) { $alias = $value; return operand_sink($alias); }

function operand_member($value, $other) { return $value->perform($other); }

class OperandSelf { public function operand_instance($other) { return $this->perform($other); } }


function operand_choice($flag, $left, $right) { return $flag ? $left : $right; }

function operand_field($box, $value) { $box->slot = $value; return $box->slot; }
function operand_element($box, $value) { $box[0] = $value; return $box[1]; }

function operand_rest(...$values) { return $values[0]; }

function operand_rest_relay($value) { return operand_rest($value); }

function operand_callback(callable $callback, $value) { return $callback($value); }

function operand_bound(callable $callback, $box, $value) { return $callback($value); }
class OperandBound { function operand_bound_target($value) { return $value; } }
