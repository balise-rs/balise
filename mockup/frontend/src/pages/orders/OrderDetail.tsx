import { useParams } from "react-router";
import { useQuery } from "@balise-rs/react";
import { orders, type OrderId } from "@/balise";

export function OrderDetail() {
  const { id } = useParams() as { id: OrderId };
  const { data, error, isLoading } = useQuery(orders.get, { id });

  if (isLoading) return <p>Loading…</p>;
  if (error) return <p>{error.message}</p>;

  return (
    <main>
      <h1>Order {data.id}</h1>
      <p>Quantity: {data.quantity}</p>
      <p>Note: {data.note ?? "none"}</p>
      <p>Created: {new Date(data.createdAt).toLocaleString()}</p>
    </main>
  );
}
