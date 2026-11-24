operand_sink() { printf "%s" "$1"; }
operand_relay() { local alias_value="$1"; operand_sink "$alias_value"; }

operand_sparse() { operand_sink "$2"; }
operand_sparse_caller() { operand_sparse "ignored" "$1"; }
