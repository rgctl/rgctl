; Tier-2 Verilog/SystemVerilog tags for rgctl (`handler = "queries"`).

(module_declaration
  (module_header (simple_identifier) @name)) @definition.module

(function_declaration
  (function_body_declaration
    (function_identifier (simple_identifier) @name))) @definition.function

(function_declaration
  (function_body_declaration
    (function_identifier
      (function_identifier (simple_identifier) @name)))) @definition.function

(task_declaration
  (task_body_declaration
    (task_identifier (simple_identifier) @name))) @definition.function

(task_declaration
  (task_body_declaration
    (task_identifier
      (task_identifier (simple_identifier) @name)))) @definition.function

(class_declaration
  (class_identifier (simple_identifier) @name)) @definition.class

(tf_call
  (simple_identifier) @name) @reference.call

(module_instantiation
  .
  (simple_identifier) @name) @reference.class

