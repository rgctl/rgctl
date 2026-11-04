const { commaSep, commaSep1, anySep1 } = require('./common.js');

exports.rules = {
  expression: $ =>
    prec.left(
      choice(
        $.binary_expression,
        prec.left(20, seq(field('object', $.expression), '.', field('attribute', $.identifier))),
        prec.left(20, seq(field('object', $.expression), '[', field('index', choice($.expression, $.slice)), ']')),
        prec.left(20, seq(field('function', $.expression), '(', commaSep($.arg), ')')),
      ),
    ),
  slice: $ => seq(optional($.expression), ':', optional($.expression), optional(seq(':', optional($.expression)))),
  binary_expression: $ =>
    choice(
      $.unary_expression,
      prec.left(seq(field('left', $.expression), field('operator', $.binary_operator), field('right', $.expression))),
      prec.left(seq(
        field('left', $.expression),
        field('operator', alias('is', $.binary_operator)),
        field('right', choice($.expression, $.builtin_test)),
      )),
      prec.left(seq(
        $.expression,
        alias(seq(optional('not'), 'in'), $.binary_operator),
        $.expression,
      )),
    ),
  builtin_test: $ =>
    prec.left(
      choice(
        'boolean',
        'callable',
        'defined',
        seq('divisibleby', $.expression),
        seq('eq', $.expression),
        'escaped',
        'even',
        'filter',
        'float',
        seq('ge', $.expression),
        seq('gt', $.expression),
        seq('in', $.expression),
        'integer',
        'iterable',
        seq('le', $.expression),
        'lower',
        seq('lt', $.expression),
        'mapping',
        seq('ne', $.expression),
        'number',
        'odd',
        seq('sameas', $.expression),
        'sequence',
        'string',
        'test',
        'undefined',
        'upper',
      ),
    ),
  assignment_expression: $ =>
    seq(
      anySep1($.identifier, '.'),
      alias('=', $.binary_operator),
      $.expression,
    ),
  in_expression: $ =>
    prec(
      1,
      seq(
        commaSep1($.identifier),
        alias('in', $.binary_operator),
        $.expression,
      ),
    ),
  binary_operator: _ =>
    choice(
      '+',
      '-',
      '*',
      '/',
      '//',
      '%',
      '**',
      '==',
      '!=',
      '>',
      '>=',
      '<',
      '<=',
      'and',
      'or',
      '|',
      '~',
    ),
  unary_expression: $ =>
    choice($.primary_expression, seq($.unary_operator, $.unary_expression)),
  unary_operator: _ => choice('not', '!', '+', '-'),
  primary_expression: $ =>
    choice(
      $.function_call,
      $.literal,
      $.inline_trans,
      $.identifier,
      seq('(', $.expression, ')'),
    ),

  function_call: $ => prec(1, seq($.identifier, '(', commaSep($.arg), ')')),
  arg: $ =>
    seq(
      optional(seq($.identifier, alias('=', $.binary_operator))),
      $.expression,
    ),
  inline_trans: $ => seq('_', '(', $.expression, ')'),
};
