import { useParams } from "react-router";
import { useMutation, useQuery } from "@balise-rs/react";
import { orders, type OrderId } from "@/balise";

export function OrderDetail() {
  const { id } = useParams() as { id: OrderId };
  const { data, error, isLoading } = useQuery(orders.get, { id });
  // Refreshes `orders.get` and `orders.list` on success.
  const cancel = useMutation(orders.cancel);

  if (isLoading) return <p>Loading…</p>;
  if (error) return <p>{error.message}</p>;

  return (
    <main>
      <h1>Order {data.id}</h1>
      <p>Quantity: {data.quantity}</p>
      <p>Note: {data.note ?? "none"}</p>
      <p>Status: {data.status}</p>
      <p>Created: {new Date(data.createdAt).toLocaleString()}</p>
      {data.status === "pending" && (
        <button onClick={() => cancel.run({ id })} disabled={cancel.isRunning}>
          Cancel order
        </button>
      )}
      {cancel.error && <p>{cancel.error.message}</p>}
    </main>
  );
}
