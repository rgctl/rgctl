def operand_sink(value):
    return value
def operand_relay(value):
    alias = value
    return operand_sink(alias)

def operand_member(value, other):
    return value.perform(other)

def operand_choice(flag, left, right):
    return left if flag else right

def operand_field(box, value):
    box.slot = value
    return box.slot
def operand_element(box, value):
    box[0] = value
    return box[1]

def operand_rest(*values: object):
 return values[0]

def operand_rest_relay(value):
 return operand_rest(value)

def operand_callback(callback, value):
 return callback(value)

def operand_bound(callback, box, value):
 return callback(value)
class OperandBound:
 def operand_bound_target(self, value):
  return value

def operand_callable_field(box, callback, value):
 box.callback = callback
 return box.callback(value)

async def operand_deferred_source(value):
 return value
async def operand_await(value):
 return await operand_deferred_source(value)
def operand_deferred_runner(value):
 return asyncio.run(operand_deferred_source(value))

def operand_yield(value, terminal):
 yield value
 return terminal
def operand_generator(value, terminal):
 iterator = operand_yield(value, terminal)
 return next(iterator)
async def operand_async_yield(value):
 yield value
async def operand_async_generator(value):
 return await anext(operand_async_yield(value))
def operand_delegated(value, terminal):
 yield from [value]
 return terminal
def operand_delegated_factory(value, terminal):
 return operand_delegated(value, terminal)
def operand_not_generator(value):
 return next([value])

def operand_computed(box, key, value):
 return getattr(box, key)(value)
class OperandConstructed:
 def __init__(self, value):
  self.slot = value
def operand_construct(value):
 return OperandConstructed(value)
def operand_constructor_alias(Factory, value):
 alias = Factory
 return alias(value)
def operand_constructor_registry(Factory, value):
 registry = [Factory]
 return registry[0](value)
def operand_not_getattr(box, key):
 return box.getattr(key)
