<?php
function operand_sink($value) { return $value; }
function operand_relay($value) { $alias = $value; return operand_sink($alias); }

function operand_member($value, $other) { return $value->perform($other); }
