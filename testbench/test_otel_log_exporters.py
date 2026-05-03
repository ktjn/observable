from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parent


class TestbenchOtelLogExporterTest(unittest.TestCase):
    def test_python_services_configure_otlp_log_exporter(self):
        services = {
            "shop-api": ROOT / "api" / "main.py",
            "shop-worker": ROOT / "worker" / "worker.py",
            "shop-loadgen": ROOT / "loadgen" / "loadgen.py",
        }

        for service, path in services.items():
            with self.subTest(service=service):
                source = path.read_text(encoding="utf-8")

                self.assertIn("OTLPLogExporter", source)
                self.assertIn("LoggerProvider(resource=resource)", source)
                self.assertIn("BatchLogRecordProcessor", source)
                self.assertIn("set_logger_provider(log_provider)", source)


if __name__ == "__main__":
    unittest.main()
