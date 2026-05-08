import { Button } from "../components/ui/button"
import { initiateLogin } from "../api/auth";

export default function LoginPage() {
  return (
    <div className="flex flex-col items-center justify-center min-h-screen bg-background text-foreground p-8 gap-6">
      <div className="text-4xl font-bold tracking-tight">Observable</div>
      <Button 
        onClick={initiateLogin}
        className="px-8 h-12"
      >
        Sign in
      </Button>
    </div>
  );
}
