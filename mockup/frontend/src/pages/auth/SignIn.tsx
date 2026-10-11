import { Link, useNavigate } from "react-router";
import { useForm } from "@balise-rs/react";
import { auth } from "@/balise";

export function SignIn() {
  const navigate = useNavigate();
  const form = useForm(auth.signIn, { onSuccess: () => navigate("/") });

  return (
    <main>
      <h1>Sign in</h1>
      <form onSubmit={form.submit}>
        <label>
          Email
          <input type="email" {...form.field("email")} />
        </label>
        <label>
          Password
          <input type="password" {...form.field("password")} />
        </label>
        {form.error && <p>{form.error.message}</p>}
        <button type="submit" disabled={form.isSubmitting}>
          Sign in
        </button>
      </form>
      <Link to="/sign-up">Create an account</Link> · <Link to="/password-reset">Forgot password?</Link>
    </main>
  );
}
