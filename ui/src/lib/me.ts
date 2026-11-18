import { useQuery } from "@tanstack/react-query";
import { api } from "@/lib/api";

/** Whether this is a team server, and whether the person may change what only admins change. */
export function useAccess() {
  const me = useQuery({ queryKey: ["me"], queryFn: api.me, staleTime: Infinity });
  const server = me.data?.mode === "server";
  return { me: me.data, server, admin: !server || me.data?.role === "admin", member: !server || me.data?.role === "admin" || me.data?.role === "member" };
}
