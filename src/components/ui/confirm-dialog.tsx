import { AlertTriangle } from 'lucide-react';

import { Alert, AlertDescription } from '../ui/alert';
import { Button } from '../ui/button';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '../ui/dialog';

interface ConfirmDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description: string;
  confirmText?: string;
  cancelText?: string;
  irreversibleWarning?: string;
  onConfirm: () => void;
  variant?: 'destructive' | 'default';
  extraActions?: React.ReactNode;
}

export default function ConfirmDialog({
  open,
  onOpenChange,
  title,
  description,
  confirmText = 'Confirm',
  cancelText = 'Cancel',
  irreversibleWarning,
  onConfirm,
  variant = 'destructive',
  extraActions,
}: ConfirmDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md" showCloseButton={false}>
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            {variant === 'destructive' && <AlertTriangle className="h-5 w-5 text-destructive" />}
            {title}
          </DialogTitle>
        </DialogHeader>
        <DialogDescription className="text-base">{description}</DialogDescription>
        {variant === 'destructive' && irreversibleWarning && (
          <Alert variant="destructive">
            <AlertDescription>{irreversibleWarning}</AlertDescription>
          </Alert>
        )}
        <DialogFooter className="flex flex-col-reverse sm:flex-row sm:justify-between gap-2">
          <div className="flex gap-2">
            <Button
              variant="outline"
              onClick={() => {
                onOpenChange(false);
              }}
            >
              {cancelText}
            </Button>
            <Button
              variant={variant}
              onClick={() => {
                onConfirm();
                onOpenChange(false);
              }}
            >
              {confirmText}
            </Button>
          </div>
          {extraActions}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
