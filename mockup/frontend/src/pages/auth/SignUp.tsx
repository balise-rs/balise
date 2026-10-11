import { useNavigate } from "react-router";
import { useForm } from "@balise-rs/react";
import { auth } from "@/balise";

export function SignUp() {
  const navigate = useNavigate();
  const form = useForm(auth.signUp, { onSuccess: () => navigate("/") });

  return (
    <main>
      <h1>Create an account</h1>
      <form onSubmit={form.submit}>
        <label>
          Email
          <input type="email" {...form.field("email")} />
        </label>
        {form.errors.email && <p>{form.errors.email}</p>}
        <label>
          Password
          <input type="password" {...form.field("password")} />
        </label>
        {form.errors.password && <p>{form.errors.password}</p>}
        <button type="submit" disabled={form.isSubmitting}>
          Create account
        </button>
      </form>
    </main>
  );
}
