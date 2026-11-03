const gram = require('./upstream/grammar.js');
const { defineBlocks } = require('./upstream/common/blocks.js');

function directive($, rule) {
  return seq(choice('{%', '{%-', '{%+'), rule, choice('-%}', '%}', '+%}'));
}

module.exports = grammar({
  name: 'jinja',
  extras: _ => [/\s/],
  externals: $ => [$.raw_start, $._raw_char, $.raw_end, $._inline_words],
  conflicts: $ => [
    [$.primary_expression, $.call_statement],
    [$.number_literal, $.unary_operator],
    [$.if_block],
    [$.elif_block],
    [$.else_block],
    [$.for_block],
    [$.block_block],
    [$.with_block],
    [$.filter_block],
    [$.macro_block],
    [$.call_block],
    [$.set_block],
    [$.trans_block],
    [$.pluralize_block],
    [$.autoescape_block]
  ],
  rules: {
    source: $ => repeat(choice($._node)),
    _node: $ => choice(
      $.statement,
      $.render_expression,
      $.content,
      $.raw_block,
      $.comment,
      $.if_block,
      $.for_block,
      $.macro_block,
      $.filter_block,
      $.call_block,
      $.set_block,
      $.block_block,
      $.with_block,
      $.trans_block,
      $.autoescape_block,
    ),
    ...gram.rules,
    ...defineBlocks(directive, $ => $._node),

    render_expression: $ =>
      seq(
        choice('{{', '{{-', '{{+'),
        optional(seq($.expression, optional($.ternary_expression))),
        choice('}}', '-}}', '+}}'),
      ),
    statement: $ => directive($, gram.rules.statement($)),

    raw_block: $ =>
      seq($.raw_start, alias(repeat($._raw_char), $.raw_body), $.raw_end),
    inline: $ =>
      seq(
        '# ',
        choice(
          'if',
          'endfor',
          'elif',
          'else',
          'endif',
          'endblock',
          'endwith',
          'endfilter',
          'endmacro',
          'endcall',
          'endset',
          'endtrans',
          'continue',
          'break',
          'debug',
          'endautoescape',
          'do',
          'include',
          'import',
          'set',
          'for',
          'with',
          'call',
          'extends',
          'macro',
          'filter',
          'block',
          'pluralize',
          'trans',
          'autoescape',
        ),
        $._inline_words,
        /\r?\n/,
      ),
  },
});
