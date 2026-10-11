import { Link } from "react-router";
import { useQuery } from "@balise-rs/react";
import { orders } from "@/balise";

export function OrderList() {
  const { data, error, isLoading, nextPage } = useQuery(orders.list, {});

  if (isLoading) return <p>Loading…</p>;
  if (error) return <p>{error.message}</p>;

  return (
    <main>
      <h1>Orders</h1>
      <Link to="/orders/new">New order</Link>
      <ul>
        {data.items.map((order) => (
          <li key={order.id}>
            <Link to={`/orders/${order.id}`}>
              {order.quantity} × {order.note ?? "no note"}
            </Link>
          </li>
        ))}
      </ul>
      {data.hasMore && <button onClick={nextPage}>More</button>}
    </main>
  );
}
