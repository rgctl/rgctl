function operand_sink(Integer $value) >> Integer { $value }
function operand_relay(Integer $value) >> Integer { $alias = $value
 operand_sink($alias) }


function operand_container(Integer $value) >> Array { $items = [$value]
 $items }
