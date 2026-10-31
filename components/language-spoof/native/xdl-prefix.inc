// Copyright (c) 2020-2025 HexHacking Team
// SPDX-License-Identifier: MIT
// Adapted from xDL's xdl_dsym: prefix matching for LSPlant's ART symbol resolver.
// Included in the pinned xdl.c so ELF loading and address resolution stay in xDL.
void *duck_xdl_dsym_prefix(void *handle, const char *prefix) {
    if (!handle || !prefix || !*prefix) return NULL;
    xdl_t *self = (xdl_t *)handle;
    xdl_ensure_symtab_loaded(self);
    if (!self->symtab) return NULL;
    size_t length = strlen(prefix);
    for (size_t i = 0; i < self->symtab_cnt; ++i) {
        ElfW(Sym) *symbol = self->symtab + i;
        if (!XDL_SYMTAB_IS_EXPORT_SYM(symbol->st_shndx) || symbol->st_name >= self->strtab_sz) continue;
        if (length >= self->strtab_sz - symbol->st_name) continue;
        if (strncmp(self->strtab + symbol->st_name, prefix, length) == 0)
            return xdl_resolve_symbol_address(self, symbol, NULL);
    }
    return NULL;
}
