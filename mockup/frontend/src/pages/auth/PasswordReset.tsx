import { useSearchParams } from "react-router";
import { useForm } from "@balise-rs/react";
import { auth } from "@/balise";

// Without a token: asks for the email. With the token from the email: sets the new password.
export function PasswordReset() {
  const [params] = useSearchParams();
  const token = params.get("token");
  const request = useForm(auth.requestPasswordReset);
  const reset = useForm(auth.resetPassword, { values: { token: token ?? "" } });

  if (!token) {
    return (
      <main>
        <h1>Reset your password</h1>
        {request.isSuccess ? (
          <p>If this email has an account, a reset link is on its way.</p>
        ) : (
          <form onSubmit={request.submit}>
            <input type="email" {...request.field("email")} />
            <button type="submit">Send the link</button>
          </form>
        )}
      </main>
    );
  }

  return (
    <main>
      <h1>Choose a new password</h1>
      <form onSubmit={reset.submit}>
        <input type="password" {...reset.field("password")} />
        {reset.errors.password && <p>{reset.errors.password}</p>}
        <button type="submit">Save</button>
      </form>
    </main>
  );
}
