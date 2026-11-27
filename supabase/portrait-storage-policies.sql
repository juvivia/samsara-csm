-- Execute este arquivo no SQL Editor do Supabase.
-- O bucket "portraits" deve existir e permanecer PRIVADO.
-- Caminho dos arquivos:
-- {character_id}/{uploader_user_id}/portrait.{jpg|png|webp}
--
-- Jogadores continuam limitados às próprias fichas e aos próprios arquivos.
-- A conta Mestre pode visualizar, enviar, substituir e excluir retratos de
-- qualquer ficha, sem tornar o bucket público.

drop policy if exists "Character owners can view their portraits" on storage.objects;
drop policy if exists "Character owners can upload their portraits" on storage.objects;
drop policy if exists "Character owners can update their portraits" on storage.objects;
drop policy if exists "Character owners can delete their portraits" on storage.objects;
drop policy if exists "Master can manage all portraits" on storage.objects;

create policy "Character owners can view their portraits"
on storage.objects
for select
to authenticated
using (
    bucket_id = 'portraits'
    and (
        (
            (storage.foldername(name))[2] = (select auth.uid())::text
            and exists (
                select 1
                from public.characters as c
                where c.id::text = (storage.foldername(name))[1]
                  and c.user_id = (select auth.uid())
            )
        )
        or (select auth.uid()) = 'ac5dd4c6-6aa7-47a6-8192-703b44ef3961'::uuid
    )
);

create policy "Character owners can upload their portraits"
on storage.objects
for insert
to authenticated
with check (
    bucket_id = 'portraits'
    and (
        (
            (storage.foldername(name))[2] = (select auth.uid())::text
            and exists (
                select 1
                from public.characters as c
                where c.id::text = (storage.foldername(name))[1]
                  and c.user_id = (select auth.uid())
            )
        )
        or (select auth.uid()) = 'ac5dd4c6-6aa7-47a6-8192-703b44ef3961'::uuid
    )
);

create policy "Character owners can update their portraits"
on storage.objects
for update
to authenticated
using (
    bucket_id = 'portraits'
    and (
        (
            (storage.foldername(name))[2] = (select auth.uid())::text
            and exists (
                select 1
                from public.characters as c
                where c.id::text = (storage.foldername(name))[1]
                  and c.user_id = (select auth.uid())
            )
        )
        or (select auth.uid()) = 'ac5dd4c6-6aa7-47a6-8192-703b44ef3961'::uuid
    )
)
with check (
    bucket_id = 'portraits'
    and (
        (
            (storage.foldername(name))[2] = (select auth.uid())::text
            and exists (
                select 1
                from public.characters as c
                where c.id::text = (storage.foldername(name))[1]
                  and c.user_id = (select auth.uid())
            )
        )
        or (select auth.uid()) = 'ac5dd4c6-6aa7-47a6-8192-703b44ef3961'::uuid
    )
);

create policy "Character owners can delete their portraits"
on storage.objects
for delete
to authenticated
using (
    bucket_id = 'portraits'
    and (
        (
            (storage.foldername(name))[2] = (select auth.uid())::text
            and exists (
                select 1
                from public.characters as c
                where c.id::text = (storage.foldername(name))[1]
                  and c.user_id = (select auth.uid())
            )
        )
        or (select auth.uid()) = 'ac5dd4c6-6aa7-47a6-8192-703b44ef3961'::uuid
    )
);
