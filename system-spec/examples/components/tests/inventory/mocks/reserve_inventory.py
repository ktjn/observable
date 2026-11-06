class ProgrammableInventory:
    def __init__(self):
        self.reset()

    def reset(self):
        self.outcomes = []
        self.calls = []

    def configure(self, *outcomes):
        self.outcomes = list(outcomes)

    def reserve(self, product_id, qty):
        self.calls.append((product_id, qty))
        if not self.outcomes:
            raise AssertionError("Unconfigured inventory interaction")
        result = self.outcomes.pop(0)
        if isinstance(result, BaseException):
            raise result
        return result


mock = ProgrammableInventory()
reserve = mock.reserve
