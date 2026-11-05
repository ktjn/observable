import pytest

def pytest_configure(config):
    config.addinivalue_line("markers", "verifies(ref): 'requirement@version' the test verifies (recorded as a JUnit property for --check-report)")

def pytest_collection_modifyitems(items):
    for i in items:
        i.user_properties.append(("test", i.nodeid))
        if m := i.get_closest_marker("verifies"): i.user_properties.append(("verifies", m.args[0]))
