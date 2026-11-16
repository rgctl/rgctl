function operand_sink(Integer $value) >> Integer { $value }
function operand_relay(Integer $value) >> Integer { $alias = $value
 operand_sink($alias) }


function operand_container(Integer $value) >> Array { $items = [$value]
 $items }

function operand_callback(String $callback, Integer $value) >> Integer { call($callback, $value) }

function operand_computed(Hash $box, String $key, Integer $value) >> Integer { call($box[$key], $value) }
function operand_resource_reference() >> Resource { File['operand.txt'] }
