import tools, _hidden
from pkg.core import outside
a = None
note = 'line\nraw'
flag = True
class A:
    a = 1
    def member(self, p: int = 2): return p
class B:
    a = 2
def K(): pass
def K(): pass
def _private(): pass
def __init__(): pass
def caller(): K()
