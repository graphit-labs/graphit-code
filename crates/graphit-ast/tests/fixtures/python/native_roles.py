import tools
from pkg.core import outside
import package as alias

BASE = 1

class Base:
    """Base documentation.
    Authored multiline text."""
    pass

@mark
class Child(Base):
    # leading run comment
    def run(self, x: int, y=2, z: str="a") -> int:
        if x and y:
            return helper(x)
        self.value = x
        return self.value

def helper(value):
    return value

def caller(arg):
    helper(arg)
    callback = lambda item: helper(item)
    return callback(arg)

def bindings(items):
    global outside_value
    for item in items:
        helper(item)
    try:
        helper()
    except Error as caught:
        caught()
    with manager() as held:
        held()
    return [helper(item) for item in items]
