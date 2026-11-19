-- SAMSARA: permissões da conta Mestre
-- Execute no SQL Editor do Supabase do projeto vinculado ao site.

-- A conta Mestre pode consultar, editar e excluir fichas de qualquer usuário.
-- As permissões existentes dos jogadores continuam valendo para as próprias fichas.
create policy "samsara_master_select_characters"
on public.characters
for select
to authenticated
using (auth.uid() = 'ac5dd4c6-6aa7-47a6-8192-703b44ef3961'::uuid);

create policy "samsara_master_update_characters"
on public.characters
for update
to authenticated
using (auth.uid() = 'ac5dd4c6-6aa7-47a6-8192-703b44ef3961'::uuid)
with check (auth.uid() = 'ac5dd4c6-6aa7-47a6-8192-703b44ef3961'::uuid);

create policy "samsara_master_delete_characters"
on public.characters
for delete
to authenticated
using (auth.uid() = 'ac5dd4c6-6aa7-47a6-8192-703b44ef3961'::uuid);

-- Permite que a conta Mestre gere links assinados e exclua retratos.
create policy "samsara_master_read_portraits"
on storage.objects
for select
to authenticated
using (
  bucket_id = 'portraits'
  and auth.uid() = 'ac5dd4c6-6aa7-47a6-8192-703b44ef3961'::uuid
);

create policy "samsara_master_delete_portraits"
on storage.objects
for delete
to authenticated
using (
  bucket_id = 'portraits'
  and auth.uid() = 'ac5dd4c6-6aa7-47a6-8192-703b44ef3961'::uuid
);
