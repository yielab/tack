"""A tiny calculator."""


def add(a, b):
    return a + b


def subtract(a, b):
    return a - b


def divide(a, b):
    return a / b
def multiply(a, b):
    return a * b
def evaluate(text):
    left, operator, right = text.split()
    a, b = float(left), float(right)
    operations = {"+": add, "-": subtract, "*": multiply, "/": divide}
    if operator not in operations:
        raise ValueError(f"unknown operator {operator}")
    return operations[operator](a, b)
