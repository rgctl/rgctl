const { commaSep, commaSep1 } = require('./common/common.js');
const literal = require('./common/literal.js');
const expression = require('./common/expression.js');

module.exports = {
  externals: $ => [$.raw_start, $._raw_char, $.raw_end],

  rules: {
    ...literal.rules,
    ...expression.rules,
    statement: $ =>
      choice(
        $.do_statement,
        $.include_statement,
        $.import_statement,
        $.set_statement,
        $.extends_statement,
        'continue',
        'break',
        'debug'
      ),
    if_statement: $ => seq('if', $.expression),
    elif_statement: $ => seq('elif', $.expression),
    else_statement: $ => 'else',
    endif_statement: $ => 'endif',

    for_statement: $ =>
      seq(
        'for',
        $.in_expression,
        optional($.ternary_expression),
        optional('recursive'),
      ),
    endfor_statement: $ => 'endfor',

    block_statement: $ => seq('block', $.identifier, optional('scoped'), optional('required')),
    endblock_statement: $ => seq('endblock', optional($.identifier)),

    with_statement: $ => seq('with', optional(commaSep1($.assignment_expression))),
    endwith_statement: $ => 'endwith',

    filter_statement: $ => seq('filter', $.expression),
    endfilter_statement: $ => 'endfilter',

    macro_statement: $ => seq('macro', $.function_call),
    endmacro_statement: $ => 'endmacro',

    call_statement: $ => seq('call', optional(seq('(', commaSep($.identifier), ')')), field('call', $.expression)),
    endcall_statement: $ => 'endcall',

    set_statement: $ =>
      seq(
        'set',
        commaSep1($.expression),
        alias('=', $.binary_operator),
        $.expression,
        optional($.ternary_expression),
      ),
    set_block_statement: $ => seq('set', commaSep1($.expression)),
    endset_statement: $ => 'endset',

    trans_statement: $ => seq('trans', optional(commaSep(choice($.identifier, $.assignment_expression)))),
    endtrans_statement: $ => 'endtrans',

    autoescape_statement: $ => seq('autoescape', optional($.boolean_literal)),
    endautoescape_statement: $ => 'endautoescape',

    pluralize_statement: $ => seq('pluralize', optional($.identifier)),

    do_statement: $ => seq('do', $.expression),
    ternary_expression: $ =>
      seq('if', $.expression, optional(seq('else', $.expression))),
    extends_statement: $ =>
      prec.right(
        seq(
          'extends',
          choice($.string_literal, $.identifier),
          optional($.ternary_expression),
        ),
      ),
    import_statement: $ =>
      choice(
        seq('import', $.string_literal, $.import_as, optional($.import_attribute)),
        seq($.import_from, 'import', commaSep1($.import_binding), optional($.import_attribute)),
      ),
    import_binding: $ => seq(field('name', $.identifier), optional(seq('as', field('alias', $.identifier)))),
    import_from: $ => seq('from', $.string_literal),
    import_as: $ => seq('as', commaSep1($.identifier)),
    import_attribute: $ => $.attribute_context,
    include_statement: $ =>
      seq(
        'include',
        choice($.string_literal, $.identifier, $.list_literal, $.tuple_literal),
        repeat($.include_attribute),
      ),
    include_attribute: $ => choice($.attribute_ignore, $.attribute_context),
    attribute_ignore: _ => seq('ignore', 'missing'),
    attribute_context: _ => seq(choice('with', 'without'), 'context'),

    content: _ => prec.right(repeat1(choice(/[^{]/, /\{[^#%]/, '#', '# '))),
    identifier: _ => /[a-zA-Z_][\w\d_]*/,
    comment: _ => seq('{#', repeat(choice(/[^#]+/, '#')), '#}'),
  },
};
