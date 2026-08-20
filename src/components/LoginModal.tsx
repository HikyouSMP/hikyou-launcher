import { motion, AnimatePresence } from "framer-motion";
import { CheckCircle2, XCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import { C } from "../theme";
import type { LoginState } from "../types";
import { ModalBackdrop } from "./ModalBackdrop";

interface LoginModalProps {
  isOpen: boolean;
  state: LoginState;
  errorMessage?: string;
  onRetry?: () => void;
  onClose?: () => void;
}

export function LoginModal({
  isOpen,
  state,
  errorMessage,
  onRetry,
  onClose,
}: LoginModalProps) {
  const { t } = useTranslation();

  return (
    <AnimatePresence>
      {isOpen && (
        <>
          {/* Backdrop */}
          <motion.div
            className="fixed inset-0 z-50"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0 }}
            onClick={onClose}
          >
            <ModalBackdrop fixed />
          </motion.div>

          {/* Modal container */}
          <div className="fixed inset-0 flex items-center justify-center z-51 pointer-events-none">
            <motion.div
              className="w-100 p-8 pointer-events-auto rounded-md bg-surface border border-b1"
              style={{ boxShadow: "0 24px 64px rgba(0,0,0,.6)" }}
              initial={{ opacity: 0, scale: 0.9 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.9 }}
            >
              {/* ── Success state ── */}
              {state === "success" && (
                <div className="flex flex-col items-center gap-4 text-center py-6">
                  <CheckCircle2 size={64} color={C.green} />
                  <div>
                    <h2 className="text-t1 text-[22px] font-bold mb-2">
                      {t("login_modal.success_title")}
                    </h2>
                    <p className="text-t2 text-[13px]">
                      {t("login_modal.success_body")}
                    </p>
                  </div>
                </div>
              )}

              {/* ── Error state ── */}
              {state === "error" && (
                <div className="flex flex-col items-center gap-4 text-center py-4">
                  <XCircle size={64} color={C.danger} />
                  <div>
                    <h2 className="text-t1 text-xl font-bold mb-2">
                      {t("login_modal.error_title")}
                    </h2>
                    <p
                      className="text-xs mb-6 max-w-75 wrap-break-word leading-normal"
                      style={{ color: "rgba(160,40,65,.9)" }}
                    >
                      {errorMessage || t("login_modal.error_unknown")}
                    </p>
                    <button
                      onClick={onRetry}
                      className="btn-phys rounded-md px-6 py-2 text-[13px] font-medium cursor-pointer text-t1 bg-hover border border-b1"
                    >
                      {t("login_modal.try_again")}
                    </button>
                  </div>
                </div>
              )}
            </motion.div>
          </div>
        </>
      )}
    </AnimatePresence>
  );
}
