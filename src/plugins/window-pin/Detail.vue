<script setup lang="ts">
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Pin } from "lucide-vue-next";
</script>

<template>
  <div class="p-4 space-y-4">
    <div class="flex items-center gap-2 mb-4">
      <Pin class="h-5 w-5" />
      <h2 class="text-lg font-semibold">Window Pin</h2>
    </div>
    <Card>
      <CardHeader>
        <CardTitle class="text-sm">Windows</CardTitle>
      </CardHeader>
      <CardContent>
        <p class="text-sm text-muted-foreground">No windows pinned. Select a window to pin it on top.</p>
        <Button class="mt-4" size="sm">Refresh Window List</Button>
      </CardContent>
    </Card>
  </div>
</template>
