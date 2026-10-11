import { BrowserRouter, Route, Routes } from "react-router";
import { SignedIn } from "@balise-rs/react";
import { OrderList } from "./pages/orders/OrderList";
import { OrderDetail } from "./pages/orders/OrderDetail";
import { OrderForm } from "./pages/orders/OrderForm";
import { SignIn } from "./pages/auth/SignIn";
import { SignUp } from "./pages/auth/SignUp";
import { PasswordReset } from "./pages/auth/PasswordReset";

export function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/sign-in" element={<SignIn />} />
        <Route path="/sign-up" element={<SignUp />} />
        <Route path="/password-reset" element={<PasswordReset />} />
        {/* Signed-out visitors are sent to /sign-in. */}
        <Route element={<SignedIn redirectTo="/sign-in" />}>
          <Route path="/" element={<OrderList />} />
          <Route path="/orders/new" element={<OrderForm />} />
          <Route path="/orders/:id" element={<OrderDetail />} />
        </Route>
      </Routes>
    </BrowserRouter>
  );
}
