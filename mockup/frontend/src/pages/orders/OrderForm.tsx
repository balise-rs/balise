import { useNavigate } from "react-router";
import { useForm } from "@balise-rs/react";
import { orders } from "@/balise";

export function OrderForm() {
  const navigate = useNavigate();
  // Validates with the rules of `CreateOrder` before sending; server errors land under their field.
  const form = useForm(orders.create, {
    onSuccess: (order) => navigate(`/orders/${order.id}`),
  });

  return (
    <main>
      <h1>New order</h1>
      <form onSubmit={form.submit}>
        <label>
          Quantity
          <input type="number" {...form.field("quantity")} />
        </label>
        {form.errors.quantity && <p>{form.errors.quantity}</p>}

        <label>
          Note
          <textarea {...form.field("note")} />
        </label>
        {form.errors.note && <p>{form.errors.note}</p>}

        <button type="submit" disabled={form.isSubmitting}>
          Create
        </button>
      </form>
    </main>
  );
}
