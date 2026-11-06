class ProgrammableCheckout:
    def __init__(self):
        self.reset()

    def reset(self):
        self.outcomes = []
        self.calls = []

    def configure(self, *outcomes):
        self.outcomes = list(outcomes)

    def place_order(self, product_id, qty, reserve):
        self.calls.append((product_id, qty))
        if not self.outcomes:
            raise AssertionError("Unconfigured checkout interaction")
        result = self.outcomes.pop(0)
        if isinstance(result, BaseException):
            raise result
        return result


mock = ProgrammableCheckout()
place_order = mock.place_order
